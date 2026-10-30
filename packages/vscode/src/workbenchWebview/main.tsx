// SPDX-License-Identifier: Apache-2.0
import { render } from 'preact';
import { useEffect, useRef, useState } from 'preact/hooks';
import { object, WorkbenchMessage, WorkbenchState, WorkbenchTurn } from '../workbenchContracts';
import { canonical, markdown } from '../workbenchRender';
import './style.css';
declare function acquireVsCodeApi(): { postMessage(message:WorkbenchMessage):void };
const vscode=acquireVsCodeApi(),send=(message:WorkbenchMessage)=>vscode.postMessage(message);
const COMMANDS=['/search','/symbols','/refs','/read','/outline','/files','/info','/imports','/facts','/changes --since','/status','/diagnostics','/capabilities','/context','/relationships','/help'];
const RECIPES=[['definition','Explore a definition'],['documents','Collect document evidence'],['impact','Find possible impact'],['tests','Find test mentions'],['component','Explore a component'],['readiness','Check retrieval readiness']];
function ToolForm({state,scope}:{state:WorkbenchState;scope:string}){
    const [tool,setTool]=useState('search'),[values,setValues]=useState<Record<string,string>>({});
    const schema=object(state.operations?.[tool]),properties=object(schema.properties),required=Array.isArray(schema.required)?schema.required.map(String):[];
    const submit=(execute:boolean)=>{const args=Object.fromEntries(Object.entries(values).filter(([,value])=>value!=='').map(([key,value])=>[key,object(properties[key]).type==='integer'?Number(value):value]));send({type:'run',input:`${tool} form`,prepare:!execute,pathPrefix:scope,plan:{steps:[{id:'form',tool,args}],path_prefix:scope}});};
    return <details class="tool-form" open><summary>Native tool form</summary><label>Operation<select value={tool} onChange={e=>{setTool(e.currentTarget.value);setValues({});}}>{Object.keys(state.operations??{}).map(name=><option value={name}>{name}</option>)}</select></label><form onSubmit={e=>{e.preventDefault();submit(false);}}>
        {Object.entries(properties).map(([key,raw])=>{const spec=object(raw);return <label>{key}{required.includes(key)?' *':''}{Array.isArray(spec.enum)?<select value={values[key]??''} onChange={e=>setValues({...values,[key]:e.currentTarget.value})}><option value="">Default</option>{spec.enum.map(value=><option value={String(value)}>{String(value)}</option>)}</select>:<input required={required.includes(key)} type={spec.type==='integer'?'number':'text'} min={typeof spec.minimum==='number'?spec.minimum:undefined} max={typeof spec.maximum==='number'?spec.maximum:undefined} maxLength={4096} value={values[key]??''} onInput={e=>setValues({...values,[key]:e.currentTarget.value})}/>}</label>;})}
        <button disabled={state.busy} type="submit">Prepare form</button><button disabled={state.busy} type="button" onClick={e=>{if(e.currentTarget.form?.reportValidity()){submit(true);}}}>Run form</button>
    </form></details>;
}
function Turn({turn,state}:{turn:WorkbenchTurn;state:WorkbenchState}){
    const [format,setFormat]=useState<'structured'|'json'|'markdown'>(turn.format),[show,setShow]=useState(20),[filter,setFilter]=useState(''),[selected,setSelected]=useState<string[]>([]),[edit,setEdit]=useState(false),[draft,setDraft]=useState(''),[issue,setIssue]=useState(''),[group,setGroup]=useState(false);
    const envelope=turn.envelope??{},plan=object(envelope.plan),cards=state.cards[turn.id]??[];
    const rows=cards.filter(c=>`${c.path} ${c.text}`.toLowerCase().includes(filter.toLowerCase()));
    if(group){rows.sort((a,b)=>a.path.localeCompare(b.path)||a.start-b.start);}
    const publication=object(state.status?.publication).token,old=typeof publication==='string'&&typeof envelope.publication==='string'&&publication!==envelope.publication;
    const runPlan=(prepare:boolean)=>{try{const parsed=JSON.parse(draft);if(prepare){parsed.digest='';}send({type:'run',input:turn.input,prepare,plan:parsed});setIssue('');}catch{setIssue('Enter a valid JSON plan.');}};
    return <article class="turn">
        <div class="input">{turn.input}</div><time>{new Date(turn.at).toLocaleTimeString()}</time>
        {old&&<p class="warning">From an older index</p>}
        {turn.error&&<p class="error" role="alert">{turn.error}</p>}
        {turn.trimmed&&<p class="warning">Saved result trimmed. Re-run to collect current evidence.</p>}

        {!!turn.envelope&&<>
            <details><summary>Plan and evidence details</summary><button disabled={state.busy} onClick={()=>send({type:'rerun',turn:turn.id})}>Re-run with current evidence</button><pre>{canonical(plan)}</pre><button onClick={()=>{setEdit(!edit);setDraft(canonical(plan));}}>Edit plan</button><button onClick={()=>send({type:'remember',turn:turn.id})}>Remember accepted inputs</button><button onClick={()=>send({type:'saveStrategy',turn:turn.id})}>Save compatible inputs to Strategies…</button></details>
            {Array.isArray(plan.steps)&&plan.steps.length>0&&['prepared','needs_input'].includes(String(envelope.status))&&<button disabled={state.busy} onClick={()=>{setEdit(true);setDraft(canonical(plan));}}>Review proposed plan</button>}
            {edit&&<div class="editor"><label>Plan JSON<textarea value={draft} onInput={e=>setDraft(e.currentTarget.value)} spellcheck={false}/></label><button onClick={()=>runPlan(true)}>Prepare edited plan</button><button disabled={state.busy} onClick={()=>runPlan(false)}>Run plan</button>{issue&&<p role="alert">{issue}</p>}</div>}
            {Array.isArray(envelope.requires_input)&&envelope.requires_input.length>0&&<div class="warning"><p>{envelope.requires_input.map(r=>String(object(r).reason??r)).join(' · ')}</p>{envelope.requires_input.flatMap(raw=>{const requirement=object(raw);return Array.isArray(requirement.choices)&&['name','path'].includes(String(requirement.field))?requirement.choices.map(choice=><button disabled={state.busy} onClick={()=>{const revised=JSON.parse(canonical(plan));revised.digest='';const target=revised.steps.find((s:any)=>s.id===requirement.step);if(target){if(requirement.field==='path'){const source=revised.steps.find((s:any)=>s.id===target.bind?.[0]?.from);if(source){source.args.path=choice;}}else{target.args.name=choice;}send({type:'run',input:turn.input,prepare:false,plan:revised});}}}>Use {String(choice)}</button>):[];})}<button disabled={state.busy} onClick={()=>send({type:'run',input:`/search ${JSON.stringify(turn.input)}`,prepare:true})}>Prepare text search</button><button disabled={state.busy} onClick={()=>send({type:'run',input:'/status',prepare:false})}>Index status</button><button disabled={state.busy} onClick={()=>send({type:'run',input:'/diagnostics',prepare:false})}>Diagnostics</button></div>}
            <section class="result"><header><span>{String(envelope.status)} · {cards.length} retained records</span><div role="group" aria-label="Output format">{(['structured','json','markdown'] as const).map(f=><button aria-pressed={format===f} onClick={()=>setFormat(f)}>{f}</button>)}</div></header>
                {Array.isArray(envelope.warnings)&&envelope.warnings.map(w=><p class="note">{String(w)}</p>)}
                {Array.isArray(envelope.completeness)&&envelope.completeness.length>0&&<p class="warning">Incomplete: {envelope.completeness.map(String).join('; ')}</p>}
                {format==='structured'?<>
                    {cards.length>0&&<details class="result-tools"><summary>Filter and group evidence{filter?` · ${rows.length} matches`:''}</summary><label class="filter">Filter retained evidence<input value={filter} onInput={e=>setFilter(e.currentTarget.value)}/></label><button aria-pressed={group} onClick={()=>setGroup(!group)}>Group by file</button><p class="counts">Showing {Math.min(show,rows.length)} of {rows.length} matching retained records.</p></details>}
                    {envelope.truncated===true&&<p class="warning">Upstream or output omissions reported.</p>}
                    {rows.slice(0,show).map(card=><div class="card" key={card.id}>
                        <div class="card-title"><input type="checkbox" aria-label={`Select ${card.path}:${card.start}`} checked={selected.includes(card.id)} onChange={e=>setSelected(e.currentTarget.checked?[...selected,card.id]:selected.filter(id=>id!==card.id))}/><button class="source" onClick={()=>send({type:'open',turn:turn.id,card:card.id})}>{card.path}:{card.start}–{card.end}</button></div>
                        <pre>{card.text}</pre><details class="source-details"><summary>Source details</summary><small>Step: {card.step}</small><pre>{canonical({location:card.location,sourceHash:card.hash,publication:card.publication})}</pre></details><button disabled={state.busy} onClick={()=>send({type:'context',turn:turn.id,card:card.id})}>Expand context</button>
                    </div>)}
                    {rows.length>show&&<button onClick={()=>setShow(show+20)}>Show 20 more retained records</button>}
                    {(state.continuations?.[turn.id]??[]).map(page=><button disabled={state.busy} onClick={()=>send({type:'continue',turn:turn.id,step:page.id})}>Fetch next {page.tool} page · {page.callsRemaining} calls remain</button>)}
                    {!cards.length&&<pre>{canonical(envelope.steps??envelope)}</pre>}
                </>:<pre>{format==='json'?canonical(envelope):markdown(turn)}</pre>}
                <footer><button onClick={()=>send({type:'copy',turn:turn.id,format:format==='json'?'json':'markdown'})}>Copy {format==='json'?'JSON':'Markdown'}</button>
                    {selected.length>0&&<button onClick={()=>send({type:'copy',turn:turn.id,format:format==='json'?'json':'markdown',cards:selected})}>Copy selected ({selected.length})</button>}
                    <button onClick={()=>send({type:'export',turn:turn.id,format:format==='json'?'json':'markdown'})}>Save export…</button>
                </footer>
            </section>
        </>}
        {!turn.envelope&&<button disabled={state.busy} onClick={()=>send({type:'rerun',turn:turn.id})}>Re-run with current evidence</button>}
    </article>;
}
type Menu = 'history' | 'settings' | 'add';
function Icon({name}:{name:'plus'|'history'|'settings'|'new'|'send'|'stop'}){
    const paths={"plus":"M12 4v16M4 12h16","history":"M5 7 9 3h7l5 5v8l-5 5H8l-5-5M3 6v6h6M12 7v5l4 2","settings":"M3 6h18M3 12h18M3 18h18M7 3l3 3-3 3-3-3zM17 9l3 3-3 3-3-3zM9 15l3 3-3 3-3-3z","new":"M4 21V7l4-4h8l4 4v14zM9 12h6M12 9v6","send":"m6 5 7 7-7 7m7-14 7 7-7 7","stop":"M7 4h10l3 3v10l-3 3H7l-3-3V7z"};
    return <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="square" stroke-linejoin="miter" aria-hidden="true"><path d={paths[name]}/><path class="icon-accent" d={name==='send'?'m17 9 3 3-3 3':'m19 3 2 2-2 2'}/></svg>;
}
function App(){
    const [state,setState]=useState<WorkbenchState>({projects:[],recent:[],cards:{},busy:false,save:'',persistence:true}),[input,setInput]=useState(''),[recipe,setRecipe]=useState(''),[menu,setMenu]=useState<Menu>(),[history,setHistory]=useState(-1),[scope,setScope]=useState(''),[title,setTitle]=useState('');
    const composer=useRef<HTMLTextAreaElement>(null),menuTrigger=useRef<string>(),openMenu=useRef<Menu>();
    openMenu.current=menu;
    const toggle=(next:Menu,event:Event)=>{menuTrigger.current=(event.currentTarget as HTMLElement).getAttribute('aria-controls')??undefined;setMenu(menu===next?undefined:next);setTitle(state.session?.title??'');};
    useEffect(()=>{const listener=(event:MessageEvent)=>{if(event.data?.type==='state'){setState(event.data.state);}else if(event.data?.type==='prefill'&&typeof event.data.input==='string'){setInput(event.data.input);composer.current?.focus();}};window.addEventListener('message',listener);send({type:'ready'});return()=>window.removeEventListener('message',listener);},[]);
    useEffect(()=>{const escape=(event:KeyboardEvent)=>{if(event.key==='Escape'&&openMenu.current){setMenu(undefined);requestAnimationFrame(()=>{document.querySelector<HTMLButtonElement>(`button[aria-controls="${menuTrigger.current}"]`)?.focus();});}},outside=(event:PointerEvent)=>{if(!(event.target as Element).closest('.menu-anchor'))setMenu(undefined);};document.addEventListener('keydown',escape);document.addEventListener('pointerdown',outside);return()=>{document.removeEventListener('keydown',escape);document.removeEventListener('pointerdown',outside);};},[]);
    const run=(prepare=false)=>{if(!state.busy&&(input.trim()||recipe)){send({type:'run',input,prepare,pathPrefix:scope,...(recipe?{recipe}:{})});setInput('');setHistory(-1);setMenu(undefined);}};
    const action=(message:WorkbenchMessage)=>{send(message);setMenu(undefined);};
    return <main>
        <header class="toolbar">
            <select class="project" aria-label="Project" title="Select project" value={state.project??''} onChange={e=>{action({type:'project',id:e.currentTarget.value});setRecipe('');setScope('');}}><option value="" disabled>Choose project</option>{state.projects.map(p=><option value={p.id}>{p.name}</option>)}</select>
            <div class="menu-anchor"><button class="icon-button" title="Session history" aria-label="Session history" aria-expanded={menu==='history'} aria-controls="session-history" onClick={e=>toggle('history',e)}><Icon name="history"/></button>
                {menu==='history'&&<section class="popover" id="session-history" aria-label="Session history"><h2>History</h2>{!state.recent.length&&<p>No saved sessions yet.</p>}{state.recent.map(s=><div class="history-row"><button class="history-item" onClick={()=>action({type:'recent',id:s.id})}>{s.title}<small>{s.turns} turns · {new Date(s.updatedAt).toLocaleDateString()}</small></button><button class="icon-button" aria-label={`Delete ${s.title}`} title="Delete session" onClick={()=>action({type:'delete',id:s.id})}>×</button></div>)}</section>}
            </div>
            <div class="menu-anchor"><button class="icon-button" title="Settings and session actions" aria-label="Settings and session actions" aria-expanded={menu==='settings'} aria-controls="session-settings" onClick={e=>toggle('settings',e)}><Icon name="settings"/></button>
                {menu==='settings'&&<section class="popover" id="session-settings" aria-label="Settings and session actions"><h2>Session</h2><label>Title<input maxLength={80} value={title} onInput={e=>setTitle(e.currentTarget.value)}/></label><button onClick={()=>action({type:'rename',title})}>Rename</button><button onClick={()=>action({type:'fork'})}>Fork session</button><button onClick={()=>action({type:'copy',format:'markdown'})}>Copy session as Markdown</button><button onClick={()=>action({type:'copy',format:'json'})}>Copy session as JSON</button><button onClick={()=>action({type:'export',format:'json'})}>Export session…</button>{state.save&&<p class="note">{state.save}</p>}<button onClick={()=>action({type:'retrySave'})}>Retry saving</button><details><summary>Index readiness</summary><button onClick={()=>send({type:'refresh'})}>Refresh readiness</button><p class="note">Indexed observations; live-file freshness unverified.</p><pre>{state.status?canonical(state.status):'Readiness not checked'}</pre></details><button onClick={()=>action({type:'manage'})}>AIRK Management</button><button onClick={()=>action({type:'help'})}>Workbench help</button><details><summary>Delete sessions</summary>{state.session&&<button onClick={()=>action({type:'delete',id:state.session!.id})}>Delete this session</button>}<button onClick={()=>action({type:'clear'})}>Clear project sessions…</button></details></section>}
            </div>
            <button class="icon-button" title="New session" aria-label="New session" onClick={()=>{action({type:'new'});setInput('');composer.current?.focus();}}><Icon name="new"/></button>
        </header>
        {state.notice&&<p class="notice" role="status">{state.notice}</p>}
        <div class="transcript">{!state.session?.turns.length?<section class="welcome"><h1>Explore your project</h1><p>Search indexed files and collect cited evidence.</p><div class="suggestions">{['/search retry --mode lexical','/symbols EvidenceClient','/files'].map(example=><button onClick={()=>{setInput(example);composer.current?.focus();}}>{example}</button>)}</div>{!state.projects.length&&<p>Open a trusted workspace to get started. <button onClick={()=>action({type:'manage'})}>Open Management</button></p>}</section>:state.session.turns.map(turn=><Turn key={turn.id} turn={turn} state={state}/>)}</div>
        <div class="composer">
            {(scope||recipe)&&<div class="compose-context">{scope&&<button title="Clear scope" onClick={()=>setScope('')}>{scope} ×</button>}{recipe&&<button title="Clear recipe" onClick={()=>setRecipe('')}>{RECIPES.find(([id])=>id===recipe)?.[1]} ×</button>}</div>}
            {input.startsWith('/')&&!input.includes(' ')&&<div class="commands" aria-label="Commands">{COMMANDS.filter(c=>c.startsWith(input)).map(c=><button onClick={()=>{setInput(`${c} `);composer.current?.focus();}}>{c}</button>)}</div>}
            <textarea ref={composer} maxLength={4096} aria-label="Request indexed evidence" placeholder={recipe?'Enter a name or statement for this recipe':'Ask about this project, or type /'} value={input} onInput={e=>{setInput(e.currentTarget.value);setHistory(-1);}} onKeyDown={e=>{
                if(e.key==='Enter'&&!e.shiftKey&&!e.isComposing){e.preventDefault();run();}
                if(e.key==='ArrowUp'&&!input&&state.session?.turns.length){e.preventDefault();const next=history<0?state.session.turns.length-1:Math.max(0,history-1);setHistory(next);setInput(state.session.turns[next].input);}
            }}/>
            <div class="compose-actions"><div class="menu-anchor add-anchor"><button class="icon-button add-button" title="Add scope, recipe or tool" aria-label="Add scope, recipe or tool" aria-expanded={menu==='add'} aria-controls="evidence-options" onClick={e=>toggle('add',e)}><Icon name="plus"/></button>
                {menu==='add'&&<section class="popover above" id="evidence-options" aria-label="Evidence options"><h2>Collect evidence</h2><label>Scope<input aria-label="Literal collection-relative path prefix" placeholder="Project root, or src/component/" value={scope} onInput={e=>setScope(e.currentTarget.value)}/></label><label>Recipe<select value={recipe} onChange={e=>setRecipe(e.currentTarget.value)}><option value="">Command or supported request</option>{RECIPES.map(([id,label])=><option value={id}>{label}</option>)}</select></label><button disabled={!state.project||state.busy||!input.trim()&&!recipe} onClick={()=>run(true)}>Prepare plan before running</button><button disabled={!state.project} onClick={()=>send({type:'forms'})}>Tool forms</button>{state.operations&&<ToolForm state={state} scope={scope}/>}<button onClick={()=>action({type:'help'})}>Commands and help</button></section>}
            </div><small title={state.save||'Requests read the local index'}>Local index · Read-only (no generative AI used in AIRK Workbench)</small>{state.busy?<button class="icon-button send-button" aria-label="Stop collecting evidence" title="Stop" onClick={()=>send({type:'stop'})}><Icon name="stop"/></button>:<button class="icon-button send-button" aria-label="Run request" title="Run · Enter (Shift+Enter for a new line)" disabled={!state.project||!input.trim()&&!recipe} onClick={()=>run()}><Icon name="send"/></button>}</div>
        </div><div class="sr-only" aria-live="polite">{state.busy?'Collecting evidence':'Ready'}</div>
    </main>;
}
render(<App/>,document.getElementById('root')!);
