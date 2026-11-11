// SPDX-License-Identifier: Apache-2.0
import { publicationRelationLabel } from '../publications';
import { render } from 'preact';
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'preact/hooks';
import { applyWorkbenchUpdate, defaultWorkbenchFormDraft, normalizeWorkbenchFormDraft, object, parseWorkbenchMessage, toolFormArguments, toolFormFields, Value, WorkbenchFormDraft, WorkbenchMessage, WorkbenchState, WorkbenchTurn } from '../workbenchContracts';
import {verificationLabel} from '../sourceVerification';
import { canonical, discoveryDetails, markdown, searchExecutionLabel } from '../workbenchRender';
import './style.css';
declare function acquireVsCodeApi(): { postMessage(message:WorkbenchMessage):void };
const vscode=acquireVsCodeApi(),send=(message:WorkbenchMessage)=>vscode.postMessage(message);
const COMMANDS=['/search','/symbols','/code-facts','/refs','/read','/outline','/files','/info','/imports','/facts','/changes --since','/status','/diagnostics','/capabilities','/context','/relationships','/help'];
const RECIPES=[['definition','Explore a definition'],['documents','Collect document evidence'],['impact','Find possible impact'],['tests','Find test mentions'],['component','Explore a component'],['readiness','Check retrieval readiness']];
interface ToolFormProps {
    state:WorkbenchState;issue:string;currentDraft:()=>WorkbenchFormDraft|undefined;
    onSelect:(tool:string)=>void;onField:(tool:string,name:string,value:string)=>void;onReset:(tool:string)=>void;onIssue:(issue:string)=>void;
}
function ToolForm({state,issue,currentDraft,onSelect,onField,onReset,onIssue}:ToolFormProps){
    const draft=normalizeWorkbenchFormDraft(state.session?.formDraft)??defaultWorkbenchFormDraft(),tool=draft.activeTool,values=draft.values[tool]??{};
    const schema=object(state.operations?.[tool]),fields=toolFormFields(schema);
    const submit=(execute:boolean)=>{try{const latest=currentDraft();if(!latest||latest.activeTool!==tool)throw new Error('Choose the tool form in the current session.');const args=toolFormArguments(schema,latest.values[tool]??{}),scope=latest.scope;send({type:'run',input:`${tool} form`,prepare:!execute,pathPrefix:scope,plan:{steps:[{id:'form',tool,args}],path_prefix:scope}});onIssue('');}catch(error){onIssue(error instanceof Error?error.message:'Review the form values.');}};
    return <details class="tool-form" open><summary>Tool form</summary><div class="tool-form-toolbar"><label>Operation<select aria-label="Operation" value={tool} onChange={e=>onSelect(e.currentTarget.value)}>{Object.keys(state.operations??{}).map(name=><option value={name}>{name}</option>)}</select></label><button class="icon-button" type="button" title="Reset this tool form" aria-label="Reset this tool form" disabled={!state.session} onClick={()=>onReset(tool)}><Icon name="reset"/></button></div><form onSubmit={e=>{e.preventDefault();submit(false);}}>
        {fields.map(field=><label key={field.name}>{field.label}{field.required?' *':''}
            {field.type==='boolean'||field.values?<select aria-label={field.label} required={field.required} value={values[field.name]??''} onChange={e=>onField(tool,field.name,e.currentTarget.value)}><option value="">{field.defaultValue!==undefined?`Default (${String(field.defaultValue)})`:'Default'}</option>{field.type==='boolean'?<><option value="true">Yes</option><option value="false">No</option></>:field.values?.map(value=><option value={value}>{value}</option>)}</select>
                :<input aria-label={field.label} required={field.required} type={field.type==='integer'?'number':'text'} placeholder={field.defaultValue!==undefined?`Default: ${String(field.defaultValue)}`:undefined} min={field.minimum} max={field.maximum} minLength={field.minLength} maxLength={field.maxLength??4096} value={values[field.name]??''} onInput={e=>onField(tool,field.name,e.currentTarget.value)}/>}
            {field.description&&<small class="form-hint">{field.description}</small>}
        </label>)}
        {issue&&<p role="alert">{issue}</p>}
        <button disabled={state.busy} type="submit">Prepare form</button><button disabled={state.busy} type="button" onClick={e=>{if(e.currentTarget.form?.reportValidity()){submit(true);}}}>Run form</button>
    </form></details>;
}
function countLabel(count:Value,unit:string):string {
    const scope=typeof count.scope==='string'?count.scope.replaceAll('_',' '):'verified scope';
    if(count.scope==='verified_lexical_candidates'&&count.complete===true&&typeof count.total_matches==='number')return `${count.total_matches} ${unit} in verified lexical candidates; whole-query total unknown`;
    if(count.complete===true&&typeof count.total_matches==='number')return `${count.total_matches} ${unit} (${scope})`;
    return `${typeof count.lower_bound==='number'?`At least ${count.lower_bound} verified ${unit}`:'Total unknown'} (${scope}; total unknown)`;
}
function DiscoveryDetails({turn,state}:{turn:WorkbenchTurn;state:WorkbenchState}){
    const details=useMemo(()=>discoveryDetails(turn.envelope??{}),[turn.envelope]);
    return <>{turn.envelope?.source_verification&&<details open><summary>{verificationLabel(turn.envelope.source_verification)}</summary><p class="note">Dated saved-byte agreement; viewing this receipt performs no new check. Unsaved editor buffers, extraction accuracy and search completeness are not certified.</p><pre>{canonical(turn.envelope.source_verification)}</pre></details>}{details.map(step=><section class="discovery-details" aria-label={`${step.tool} discovery details`} key={step.id}>
        {step.sourceVerification&&<details open><summary>{verificationLabel(step.sourceVerification)}</summary><p class="note">Saved disk bytes only, at the original observation time. Extraction, coverage, relevance and unsaved buffers remain qualified.</p><pre>{canonical(step.sourceVerification)}</pre></details>}
        {step.execution&&<details open><summary>Search execution</summary>{searchExecutionLabel(step.execution)&&<p class={step.execution.incomplete===true?'warning':'note'}>{searchExecutionLabel(step.execution)}</p>}<pre>{canonical(step.execution)}</pre></details>}
        {step.readError&&<details open><summary>Read refusal</summary><pre>{canonical({error:step.readError,...(step.requestedMode!==undefined?{requested_mode:step.requestedMode}:{}),...(step.publication!==undefined?{publication:step.publication}:{})})}</pre></details>}
        {(step.semanticReadiness||step.discoveryReadiness)&&<details><summary>Indexed coverage observations</summary><p class="note">These observations describe the retained index and are independent of this search's execution outcome.</p><pre>{canonical({...(step.semanticReadiness?{semantic_readiness:step.semanticReadiness}:{}),...(step.discoveryReadiness?{discovery_readiness:step.discoveryReadiness}:{})})}</pre></details>}
        {step.replyDetail&&<p class="note">Reply detail: {step.replyDetail==='lean'?'Lean':'Rich'}</p>}
        {step.readGuidance&&<details><summary>Guidance vocabulary and limits</summary><pre>{canonical(step.readGuidance)}</pre></details>}
        {step.guidance&&<details open><summary>Evidence guidance</summary><ul>{(Array.isArray(step.guidance.observations)?step.guidance.observations:[]).map((raw,i)=>{const row=object(raw);return <li key={i}><strong>{String(row.reason).replaceAll('_',' ')} ({String(row.subject).replaceAll('_',' ')}): </strong>{String(row.message)} <small>Next read: {String(row.next_read).replaceAll('_',' ')}. Basis: {String(row.basis)}.</small></li>;})}</ul>{step.guidance.truncated===true&&<p class="note">Additional guidance was omitted. Original status and coverage remain in the full response.</p>}<p class="note">Suggestions do not run tools or change indexing. Keep the original authorized scope.</p></details>}
        {!step.canFollow&&(step.definitions.length>0||step.suggestions.length>0)&&<p class="note">Re-run this investigation to enable follow-up actions from completed, publication-matched evidence.</p>}
        {step.count&&<p class="counts">{countLabel(step.count,step.tool==='references'?(step.countUnit??'matching_lines').replaceAll('_',' '):'records')}</p>}
        {step.coverage?.query_complete===false&&<p class="warning">Reference coverage is limited to verified lexical candidates. The whole-query total is unknown; identifier tokens split across indexed chunk boundaries can be missed.</p>}
        {step.tool==='references'&&step.coverage===undefined&&<p class="warning">Textual candidate matches do not establish complete reference coverage; the whole-query total is unknown.</p>}
        {step.coverage?.line_completeness==='includes_unverified_fragments'&&<p class="warning">Some extracted-text matches are fragments whose completeness as original source lines is unverified.</p>}
        {step.incompleteReason&&<p class="warning">Discovery is incomplete: {step.incompleteReason.replaceAll('_',' ')}.</p>}
        {step.more&&<p class="note">More scanning remains. Use Fetch next page when a continuation is available.</p>}
        {step.requiredRecordBytes!==undefined&&<p class="note">The next complete record needs {step.requiredRecordBytes} bytes.{!step.more?' No continuation is available; use a scoped source read or inspect the reported coverage limit.':''}</p>}
        {step.files.length>0&&<details><summary>Files in this page</summary><ul>{step.files.map(file=><li key={file.path}>{file.path}: {file.count} {(step.countUnit??'matching_lines').replaceAll('_',' ')}</li>)}</ul>{(step.filesTruncated||step.omittedFiles>0)&&<p class="warning">The file summary is incomplete. Inspect the full export and continuation coverage.</p>}</details>}
        {step.files.length===0&&step.filesTruncated&&<p class="warning">The file summary is incomplete at this budget. Inspect the reported continuation and coverage.</p>}
        {step.definitions.length>0&&<details open><summary>Definition pointers</summary><p class="note">Definitions are candidates from this same indexed publication. Only a bounded set of pointers is shown.</p>{step.definitions.map((hint,i)=><div key={i}><p>{String(hint.name??'Identifier')}: {countLabel(hint,'definition records')}</p>{(Array.isArray(hint.matches)?hint.matches:[]).map((raw,j)=>{const citation=object(object(raw).citation);return typeof hint.name==='string'&&typeof citation.path==='string'&&typeof object(raw).symbol_id==='string'&&String(object(raw).symbol_id).length>0?<button key={j} disabled={state.busy||!step.canFollow} onClick={()=>send({type:'followDefinition',turn:turn.id,step:step.id,name:String(hint.name),path:String(citation.path)})}>Inspect {hint.name} — {citation.path}</button>:null;})}</div>)}{step.omittedDefinitions>0&&<p class="note">Additional pointers remain in the full export.</p>}</details>}
        {step.suggestions.length>0&&<details open><summary>Written method targets</summary><p class="note">Follow a target for textual call candidates. These are not compiler-resolved callers.</p>{step.suggestions.map((target,i)=><div key={i}><button disabled={state.busy||!step.canFollow} onClick={()=>send({type:'followTarget',turn:turn.id,step:step.id,target:target.target})}>Find callers of {target.target}</button>{target.path&&<small>{target.path}</small>}{target.coverage&&<p class="note">{target.coverage}</p>}</div>)}{step.omittedSuggestions>0&&<p class="note">Additional targets remain in the full export.</p>}</details>}
    </section>)}</>;
}
function SelectAll({count,checked,mixed,onChange}:{count:number;checked:boolean;mixed:boolean;onChange:(checked:boolean)=>void}){
    const input=useRef<HTMLInputElement>(null);
    useLayoutEffect(()=>{if(input.current){input.current.indeterminate=mixed;}},[mixed]);
    return <label class="select-all" title="Select all matching results in this response"><input ref={input} type="checkbox" checked={checked} aria-checked={mixed?'mixed':checked} disabled={count===0} onChange={e=>onChange(e.currentTarget.checked)}/>Select all {count}</label>;
}
function Turn({turn,state}:{turn:WorkbenchTurn;state:WorkbenchState}){
    const [format,setFormat]=useState<'structured'|'json'|'markdown'>(turn.format),[show,setShow]=useState(20),[filter,setFilter]=useState(''),[selected,setSelected]=useState<string[]>([]),[edit,setEdit]=useState(false),[draft,setDraft]=useState(''),[issue,setIssue]=useState(''),[group,setGroup]=useState(false);
    const envelope=turn.envelope??{},plan=object(envelope.plan),cards=state.cards[turn.id]??[];
    const rows=useMemo(()=>{const matching=cards.filter(c=>`${c.path} ${c.text}`.toLowerCase().includes(filter.toLowerCase()));if(group)matching.sort((a,b)=>a.path.localeCompare(b.path)||a.start-b.start);return matching;},[cards,filter,group]);
    const matchingIds=new Set(rows.map(card=>card.id)),selectedMatches=rows.filter(card=>selected.includes(card.id)).length;
    const allSelected=rows.length>0&&selectedMatches===rows.length,mixedSelection=selectedMatches>0&&!allSelected;
    const selectMatching=(checked:boolean)=>setSelected(previous=>checked?[...new Set([...previous,...matchingIds])]:previous.filter(id=>!matchingIds.has(id)));
    const copyFormat=format==='json'?'json':'markdown',copySelection:WorkbenchMessage={type:'copy',turn:turn.id,format:copyFormat,cards:selected};
    const copySelectionAllowed=selected.length>0&&!!parseWorkbenchMessage(copySelection);
    const selectionTooLarge=selected.length>0&&!copySelectionAllowed;
    const copySelectionLabel=`Copy ${selected.length} selected results as ${copyFormat==='json'?'JSON':'Markdown'}`;
    const copySelectionTitle=selectionTooLarge?'Select fewer results or use Save export':copySelectionLabel;
    const selectionUi=format==='structured'&&cards.length>0;
    const selectionRow=()=><div class="selection-toolbar"><SelectAll count={rows.length} checked={allSelected} mixed={mixedSelection} onChange={selectMatching}/><button title={copySelectionTitle} aria-label={copySelectionLabel} disabled={!copySelectionAllowed} onClick={()=>send(copySelection)}>Copy selected ({selected.length})</button></div>;
    const planText=useMemo(()=>canonical(plan),[turn.envelope]);
    const formatted=useMemo(()=>format==='json'?canonical(envelope):format==='markdown'?markdown(turn):'', [format,turn]);
    const publicationNotice=publicationRelationLabel(state.publicationRelations?.[turn.id]??'unknown');
    const runPlan=(prepare:boolean)=>{try{const parsed=JSON.parse(draft);if(prepare){parsed.digest='';}send({type:'run',input:turn.input,prepare,plan:parsed});setIssue('');}catch{setIssue('Enter a valid JSON plan.');}};
    return <article class="turn">
        <div class="input">{turn.input}</div><time>{new Date(turn.at).toLocaleTimeString()}</time>
        {publicationNotice&&<p class="warning">{publicationNotice}</p>}
        {turn.error&&<p class="error" role="alert">{turn.error}</p>}
        {turn.trimmed&&<p class="warning">Saved result trimmed. Re-run to collect current evidence.</p>}

        {!!turn.envelope&&<>
            <details><summary>Plan and evidence details</summary><button disabled={state.busy} onClick={()=>send({type:'rerun',turn:turn.id})}>Re-run with current evidence</button><pre>{planText}</pre><button onClick={()=>{setEdit(!edit);setDraft(planText);}}>Edit plan</button><button onClick={()=>send({type:'remember',turn:turn.id})}>Remember accepted inputs</button><button onClick={()=>send({type:'saveStrategy',turn:turn.id})}>Save compatible inputs to Strategies…</button></details>
            {Array.isArray(plan.steps)&&plan.steps.length>0&&['prepared','needs_input'].includes(String(envelope.status))&&<button disabled={state.busy} onClick={()=>{setEdit(true);setDraft(planText);}}>Review proposed plan</button>}
            {edit&&<div class="editor"><label>Plan JSON<textarea value={draft} onInput={e=>setDraft(e.currentTarget.value)} spellcheck={false}/></label><button onClick={()=>runPlan(true)}>Prepare edited plan</button><button disabled={state.busy} onClick={()=>runPlan(false)}>Run plan</button>{issue&&<p role="alert">{issue}</p>}</div>}
            {Array.isArray(envelope.requires_input)&&envelope.requires_input.length>0&&<div class="warning"><p>{envelope.requires_input.map(r=>String(object(r).reason??r)).join(' · ')}</p>{envelope.requires_input.flatMap(raw=>{const requirement=object(raw);return Array.isArray(requirement.choices)&&['name','path'].includes(String(requirement.field))?requirement.choices.map(choice=><button disabled={state.busy} onClick={()=>{const revised=JSON.parse(planText);revised.digest='';const target=revised.steps.find((s:any)=>s.id===requirement.step);if(target){if(requirement.field==='path'){const source=revised.steps.find((s:any)=>s.id===target.bind?.[0]?.from);if(source){source.args.path=choice;}}else{target.args.name=choice;}send({type:'run',input:turn.input,prepare:false,plan:revised});}}}>Use {String(choice)}</button>):[];})}<button disabled={state.busy} onClick={()=>send({type:'run',input:`/search ${JSON.stringify(turn.input)}`,prepare:true})}>Prepare text search</button><button disabled={state.busy} onClick={()=>send({type:'run',input:'/status',prepare:false})}>Index status</button><button disabled={state.busy} onClick={()=>send({type:'run',input:'/diagnostics',prepare:false})}>Diagnostics</button></div>}
            <section class="result"><header><span>{String(envelope.status)} · {cards.length} retained records</span><div role="group" aria-label="Output format">{(['structured','json','markdown'] as const).map(f=><button aria-pressed={format===f} onClick={()=>setFormat(f)}>{f}</button>)}</div></header>
                {Array.isArray(envelope.warnings)&&envelope.warnings.map(w=><p class="note">{String(w)}</p>)}
                {Array.isArray(envelope.completeness)&&envelope.completeness.length>0&&<p class="warning">Incomplete: {envelope.completeness.map(String).join('; ')}</p>}
                {format==='structured'?<>
                    <DiscoveryDetails turn={turn} state={state}/>
                    {cards.length>0&&<details class="result-tools"><summary>Filter and group evidence{filter?` · ${rows.length} matches`:''}</summary><label class="filter">Filter retained evidence<input value={filter} onInput={e=>setFilter(e.currentTarget.value)}/></label><button aria-pressed={group} onClick={()=>setGroup(!group)}>Group by file</button><p class="counts">Showing {Math.min(show,rows.length)} of {rows.length} matching retained records.</p></details>}
                    {envelope.truncated===true&&<p class="warning">Upstream or output omissions reported.</p>}
                    {cards.length>0&&selectionRow()}
                    {cards.length>0&&selectionTooLarge&&<p class="note" role="status">Too many selected results to copy together. Select fewer results or use Save export.</p>}
                    {rows.slice(0,show).map(card=><div class="card" key={card.id}>
                        <div class="card-title"><input type="checkbox" aria-label={`Select ${card.path}:${card.start}`} checked={selected.includes(card.id)} onChange={e=>{const checked=e.currentTarget.checked;setSelected(previous=>checked?[...new Set([...previous,card.id])]:previous.filter(id=>id!==card.id));}}/><button class="source" onClick={()=>send({type:'open',turn:turn.id,card:card.id})}>{card.path}:{card.start}–{card.end}</button></div>
                        <pre>{card.text}</pre><details class="source-details"><summary>Source details</summary><small>Step: {card.step}</small><pre>{canonical({location:card.location,sourceHash:card.hash,publication:card.publication})}</pre></details><button disabled={state.busy} onClick={()=>send({type:'context',turn:turn.id,card:card.id})}>Expand context</button>
                    </div>)}
                    {cards.length>0&&selectionRow()}
                    {rows.length>show&&<button onClick={()=>setShow(show+20)}>Show 20 more retained records</button>}
                    {(state.continuations?.[turn.id]??[]).map(page=><button disabled={state.busy} onClick={()=>send({type:'continue',turn:turn.id,step:page.id})}>Fetch next {page.tool} page · {page.callsRemaining} calls remain{page.requiredRecordBytes!==undefined?` · needs ${page.requiredRecordBytes} bytes`:''}</button>)}
                    <details class="complete-response" open={!cards.length}><summary>Complete response</summary><pre>{canonical(envelope)}</pre></details>
                </>:<pre>{formatted}</pre>}
                <footer>{!selectionUi&&<button onClick={()=>send({type:'copy',turn:turn.id,format:format==='json'?'json':'markdown'})}>Copy {format==='json'?'JSON':'Markdown'}</button>}
                    {!selectionUi&&selected.length>0&&<button title={copySelectionTitle} disabled={!copySelectionAllowed} onClick={()=>send(copySelection)}>Copy selected ({selected.length})</button>}
                    <button onClick={()=>send({type:'export',turn:turn.id,format:format==='json'?'json':'markdown'})}>Save export…</button>
                </footer>
            </section>
        </>}
        {!turn.envelope&&(state.busy&&state.session?.turns.at(-1)?.id===turn.id?<p role="status">Collecting evidence…</p>:<button disabled={state.busy} onClick={()=>send({type:'rerun',turn:turn.id})}>Re-run with current evidence</button>)}
    </article>;
}
type Menu = 'history' | 'settings' | 'add';
function Icon({name}:{name:'plus'|'history'|'settings'|'new'|'send'|'stop'|'reset'}){
    const paths={"plus":"M12 4v16M4 12h16","history":"M5 7 9 3h7l5 5v8l-5 5H8l-5-5M3 6v6h6M12 7v5l4 2","settings":"M3 6h18M3 12h18M3 18h18M7 3l3 3-3 3-3-3zM17 9l3 3-3 3-3-3zM9 15l3 3-3 3-3-3z","new":"M4 21V7l4-4h8l4 4v14zM9 12h6M12 9v6","send":"m6 5 7 7-7 7m7-14 7 7-7 7","stop":"M7 4h10l3 3v10l-3 3H7l-3-3V7z","reset":"M4 11a8 8 0 1 1 2 7M4 5v6h6"};
    return <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="square" stroke-linejoin="miter" aria-hidden="true"><path d={paths[name]}/><path class="icon-accent" d={name==='send'?'m17 9 3 3-3 3':'m19 3 2 2-2 2'}/></svg>;
}
function App(){
    const [state,setState]=useState<WorkbenchState>({projects:[],recent:[],cards:{},busy:false,save:'',persistence:true}),[input,setInput]=useState(''),[menu,setMenu]=useState<Menu>(),[history,setHistory]=useState(-1),[title,setTitle]=useState(''),[formIssue,setFormIssue]=useState('');
    const liveState=useRef(state);liveState.current=state;
    const formDraft=normalizeWorkbenchFormDraft(state.session?.formDraft)??defaultWorkbenchFormDraft(),scope=formDraft.scope,recipe=formDraft.recipe;
    const currentDraft=():WorkbenchFormDraft|undefined=>{
        const current=liveState.current;
        if(!current.session||!state.session||current.session.id!==state.session.id||current.project!==state.project||current.session.project!==current.project||current.sessionEpoch!==state.sessionEpoch||!Number.isSafeInteger(current.sessionEpoch))return;
        return normalizeWorkbenchFormDraft(current.session.formDraft);
    };
    const changeForm=(patch:{tool?:string;field?:{name:string;value:string};scope?:string;recipe?:string},reset=false)=>{
        const current=currentDraft(),owner=liveState.current;
        if(!current||!owner.session||typeof owner.sessionEpoch!=='number'){setFormIssue('Choose the form in the current session.');return;}
        const tool=patch.tool??current.activeTool;
        if((patch.field||reset)&&tool!==current.activeTool){setFormIssue('The active form changed. Use its current controls.');return;}
        const values={...current.values};
        if(reset)delete values[tool];
        else if(patch.field)values[tool]={...values[tool],[patch.field.name]:patch.field.value};
        const nextDraft=normalizeWorkbenchFormDraft({...current,activeTool:tool,values,revision:current.revision+1,...(patch.scope!==undefined?{scope:patch.scope}:{}),...(patch.recipe!==undefined?{recipe:patch.recipe}:{})});
        if(!nextDraft){setFormIssue('These form choices exceed the saved-input limits. Shorten a field or reset an unused form.');return;}
        const fence={sessionId:owner.session.id,project:owner.session.project,epoch:owner.sessionEpoch,revision:nextDraft.revision,tool};
        const message:WorkbenchMessage=reset?{type:'resetForm',...fence}:{type:'formDraft',...fence,...(patch.field?{values:nextDraft.values[tool]}:{}),...(patch.scope!==undefined?{scope:patch.scope}:{}),...(patch.recipe!==undefined?{recipe:patch.recipe}:{})};
        if(!parseWorkbenchMessage(message)){setFormIssue('This form setting is outside the supported input limits.');return;}
        const next={...owner,session:{...owner.session,formDraft:nextDraft}};
        liveState.current=next;setState(next);setFormIssue('');send(message);
    };
    const setScope=(scope:string)=>changeForm({scope}),setRecipe=(recipe:string)=>changeForm({recipe});
    const composer=useRef<HTMLTextAreaElement>(null),transcript=useRef<HTMLDivElement>(null),menuTrigger=useRef<string>(),openMenu=useRef<Menu>();
    const latest=state.session?.turns.at(-1);
    // Reveal each new request from its query, including when results arrive.
    // Keep the question above its timestamp, plan and evidence. Status/save refreshes,
    // output controls and tab switches must leave the reader's position alone.
    useLayoutEffect(()=>{
        const container=transcript.current,turn=container?.lastElementChild;
        if(!container||!latest||!turn){return;}
        const target=turn.querySelector('.input')??turn;
        container.scrollTop+=target.getBoundingClientRect().top-container.getBoundingClientRect().top;
    },[state.session?.id,latest?.id,!!latest?.envelope,latest?.error]);
    openMenu.current=menu;
    const toggle=(next:Menu,event:Event)=>{menuTrigger.current=(event.currentTarget as HTMLElement).getAttribute('aria-controls')??undefined;setMenu(menu===next?undefined:next);setTitle(state.session?.title??'');};
    useEffect(()=>{const listener=(event:MessageEvent)=>{if(['state','status','saved','formDraft'].includes(event.data?.type)){setState(previous=>{const next=applyWorkbenchUpdate(previous,event.data);liveState.current=next;return next;});}else if(event.data?.type==='prefill'&&typeof event.data.input==='string'){setInput(event.data.input);composer.current?.focus();}};window.addEventListener('message',listener);send({type:'ready'});return()=>window.removeEventListener('message',listener);},[]);
    useEffect(()=>{setHistory(-1);setFormIssue('');},[state.session?.id]);
    useEffect(()=>{const escape=(event:KeyboardEvent)=>{if(event.key==='Escape'&&openMenu.current){setMenu(undefined);requestAnimationFrame(()=>{document.querySelector<HTMLButtonElement>(`button[aria-controls="${menuTrigger.current}"]`)?.focus();});}},outside=(event:PointerEvent)=>{if(!(event.target as Element).closest('.menu-anchor'))setMenu(undefined);};document.addEventListener('keydown',escape);document.addEventListener('pointerdown',outside);return()=>{document.removeEventListener('keydown',escape);document.removeEventListener('pointerdown',outside);};},[]);
    const run=(prepare=false)=>{const current=currentDraft();if(current&&!state.busy&&(input.trim()||current.recipe)){send({type:'run',input,prepare,pathPrefix:current.scope,...(current.recipe?{recipe:current.recipe}:{})});setInput('');setHistory(-1);setMenu(undefined);}};
    const action=(message:WorkbenchMessage)=>{send(message);if(['new','clear','fork'].includes(message.type))setInput('');setMenu(undefined);};
    return <main>
        <header class="toolbar">
            <select class="project" aria-label="Project" title="Select project" value={state.project??''} onChange={e=>action({type:'project',id:e.currentTarget.value})}><option value="" disabled>Choose project</option>{state.projects.map(p=><option value={p.id}>{p.name}</option>)}</select>
            <div class="menu-anchor"><button class="icon-button" title="Session history" aria-label="Session history" aria-expanded={menu==='history'} aria-controls="session-history" onClick={e=>toggle('history',e)}><Icon name="history"/></button>
                {menu==='history'&&<section class="popover" id="session-history" aria-label="Session history"><h2>History</h2>{!state.recent.length&&<p>No saved sessions yet.</p>}{state.recent.map(s=><div class="history-row"><button class="history-item" onClick={()=>action({type:'recent',id:s.id})}>{s.title}<small>{s.turns} turns · {new Date(s.updatedAt).toLocaleDateString()}</small></button><button class="icon-button" aria-label={`Delete ${s.title}`} title="Delete session" onClick={()=>action({type:'delete',id:s.id})}>×</button></div>)}</section>}
            </div>
            <div class="menu-anchor"><button class="icon-button" title="Settings and session actions" aria-label="Settings and session actions" aria-expanded={menu==='settings'} aria-controls="session-settings" onClick={e=>toggle('settings',e)}><Icon name="settings"/></button>
                {menu==='settings'&&<section class="popover" id="session-settings" aria-label="Settings and session actions"><h2>Session</h2><label>Title<input maxLength={80} value={title} onInput={e=>setTitle(e.currentTarget.value)}/></label><button onClick={()=>action({type:'rename',title})}>Rename</button><button onClick={()=>action({type:'fork'})}>Fork session</button><button onClick={()=>action({type:'copy',format:'markdown'})}>Copy session as Markdown</button><button onClick={()=>action({type:'copy',format:'json'})}>Copy session as JSON</button><button onClick={()=>action({type:'export',format:'json'})}>Export session…</button>{state.save&&<p class="note">{state.save}</p>}<button onClick={()=>action({type:'retrySave'})}>Retry saving</button><details><summary>Index readiness</summary><button onClick={()=>send({type:'refresh'})}>Refresh readiness</button><p class="note">Collection readiness does not verify live source files. Dated saved-source checks appear with individual results.</p><pre>{state.status?canonical(state.status):'Readiness not checked'}</pre></details><button onClick={()=>action({type:'manage'})}>AIRK Management</button><button onClick={()=>action({type:'help'})}>Workbench help</button><details><summary>Delete sessions</summary>{state.session&&<button onClick={()=>action({type:'delete',id:state.session!.id})}>Delete this session</button>}<button onClick={()=>action({type:'clear'})}>Clear project sessions…</button></details></section>}
            </div>
            <button class="icon-button" title="New session" aria-label="New session" onClick={()=>{action({type:'new'});setInput('');composer.current?.focus();}}><Icon name="new"/></button>
        </header>
        {state.notice&&<p class="notice" role="status">{state.notice}</p>}
        <div class="transcript" ref={transcript}>{!state.session?.turns.length?<section class="welcome"><h1>Explore your project</h1><p>Search indexed files and collect cited evidence.</p><div class="suggestions">{['/search retry --mode lexical','/symbols EvidenceClient','/files'].map(example=><button onClick={()=>{setInput(example);composer.current?.focus();}}>{example}</button>)}</div>{!state.projects.length&&<p>Open a trusted workspace to get started. <button onClick={()=>action({type:'manage'})}>Open Management</button></p>}</section>:state.session.turns.map(turn=><Turn key={turn.id} turn={turn} state={state}/>)}</div>
        <div class="composer">
            {(scope||recipe)&&<div class="compose-context">{scope&&<button title="Clear scope" onClick={()=>setScope('')}>{scope} ×</button>}{recipe&&<button title="Clear recipe" onClick={()=>setRecipe('')}>{RECIPES.find(([id])=>id===recipe)?.[1]} ×</button>}</div>}
            {input.startsWith('/')&&!input.includes(' ')&&<div class="commands" aria-label="Commands">{COMMANDS.filter(c=>c.startsWith(input)).map(c=><button onClick={()=>{setInput(`${c} `);composer.current?.focus();}}>{c}</button>)}</div>}
            <textarea ref={composer} maxLength={4096} aria-label="Request indexed evidence" placeholder={recipe?'Enter a name or statement for this recipe':'Ask about this project, or type /'} value={input} onInput={e=>{setInput(e.currentTarget.value);setHistory(-1);}} onKeyDown={e=>{
                if(e.key==='Enter'&&!e.shiftKey&&!e.isComposing){e.preventDefault();run();}
                if(e.key==='ArrowUp'&&!input&&state.session?.turns.length){e.preventDefault();const next=history<0?state.session.turns.length-1:Math.max(0,history-1);setHistory(next);setInput(state.session.turns[next].input);}
            }}/>
            <div class="compose-actions"><div class="menu-anchor add-anchor"><button class="icon-button add-button" title="Add scope, recipe or tool" aria-label="Add scope, recipe or tool" aria-expanded={menu==='add'} aria-controls="evidence-options" onClick={e=>toggle('add',e)}><Icon name="plus"/></button>
                {menu==='add'&&<section class="popover above" id="evidence-options" aria-label="Evidence options"><h2>Collect evidence</h2><label>Scope<input aria-label="Literal collection-relative path prefix" placeholder="Project root, or src/component/" maxLength={1024} value={scope} onInput={e=>setScope(e.currentTarget.value)}/></label><label>Recipe<select aria-label="Recipe" value={recipe} onChange={e=>setRecipe(e.currentTarget.value)}><option value="">Command or supported request</option>{RECIPES.map(([id,label])=><option value={id}>{label}</option>)}</select></label><button disabled={!state.project||state.busy||!input.trim()&&!recipe} onClick={()=>run(true)}>Prepare plan before running</button><button disabled={!state.project} onClick={()=>send({type:'forms'})}>Tool forms</button>{state.operations&&<ToolForm state={state} issue={formIssue} currentDraft={currentDraft} onSelect={tool=>changeForm({tool})} onField={(tool,name,value)=>changeForm({tool,field:{name,value}})} onReset={tool=>changeForm({tool},true)} onIssue={setFormIssue}/>}<button onClick={()=>action({type:'help'})}>Commands and help</button></section>}
            </div><small title={state.save||'Requests read the local index'}>Local index · Read-only (no generative AI used in AIRK Workbench)</small>{state.busy?<button class="icon-button send-button" aria-label="Stop collecting evidence" title="Stop" onClick={()=>send({type:'stop'})}><Icon name="stop"/></button>:<button class="icon-button send-button" aria-label="Run request" title="Run · Enter (Shift+Enter for a new line)" disabled={!state.project||!input.trim()&&!recipe} onClick={()=>run()}><Icon name="send"/></button>}</div>
        </div><div class="sr-only" aria-live="polite">{state.busy?'Collecting evidence':'Ready'}</div>
    </main>;
}
render(<App/>,document.getElementById('root')!);
