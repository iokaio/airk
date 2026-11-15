// SPDX-License-Identifier: Apache-2.0
import {useLayoutEffect,useRef,useState} from 'preact/hooks';
import {CORE_PACK,CATEGORIES,operator,compilePrompt,studioPrompt,studioDocument,studioHasWork,taskFromStudio,canAddStudio,nextId,
  validateStrategy,validateStudio,validatePlan,renderPlan,effectiveState,TASK_STATES,STUDIO_LIMIT_BYTES,STRATEGY_LIMIT_BYTES,
  type Strategy,type PlanTask,type StoredTaskState} from '../../ui/src/strategies';
import {parsePlanFile} from '../../vscode/src/explorerPlanFiles';
import {currentSession} from './session';
import {strategyWorkspaceFor,type StrategyWorkspace} from './strategyWorkspace';
import {runBrowserStrategy,browserProductVersion} from './strategyRunner';
import type {ScopedReadCapture} from './SearchScreen';

export const BROWSER_STRATEGY_ROUTES=['strategies/library','strategies/studio','strategies/planner'] as const;
export type BrowserStrategyRoute=typeof BROWSER_STRATEGY_ROUTES[number];
export const isBrowserStrategyRoute=(value:unknown):value is BrowserStrategyRoute=>typeof value==='string'&&(BROWSER_STRATEGY_ROUTES as readonly string[]).includes(value);
type Props={route:BrowserStrategyRoute;scopeIdentity?:string;disabled:boolean;capture:()=>ScopedReadCapture;active?:boolean;onNavigate?:(route:BrowserStrategyRoute)=>void};
type Pending={title:string;revision:number;kind:'studio'|'plan';apply:()=>void};
const message=(error:unknown)=>error instanceof Error?error.message:String(error);
const sessionKeys=new WeakMap<object,number>();let nextSessionKey=0;
const keyFor=(session:object)=>{let key=sessionKeys.get(session);if(key===undefined){key=++nextSessionKey;sessionKeys.set(session,key);}return key;};
function download(name:string,text:string,type='application/json'){const url=URL.createObjectURL(new Blob([text],{type}));const a=document.createElement('a');a.href=url;a.download=name;a.rel='noopener noreferrer';a.click();setTimeout(()=>URL.revokeObjectURL(url),0);}
async function copy(text:string){if(!navigator.clipboard)throw Error('Clipboard is unavailable in this browser. Select the text or download it.');await navigator.clipboard.writeText(text);}

export function BrowserStrategiesScreen(props:Props) {
  const session=currentSession();
  if(!session)return <p>Open a management session to use Strategies.</p>;
  return <StrategyWorkspaceView key={keyFor(session)} {...props} workspace={strategyWorkspaceFor(session)}/>;
}
function StrategyWorkspaceView(props:Props&{workspace:StrategyWorkspace}) {
  const owner=props.workspace,[,render]=useState(0),[error,setError]=useState(''),[notice,setNotice]=useState(''),[busy,setBusy]=useState(false),[json,setJson]=useState(''),[pending,setPending]=useState<Pending|null>(null),[confirmTask,setConfirmTask]=useState<{id:string;revision:number}|null>(null);
  const [category,setCategory]=useState('all'),[filter,setFilter]=useState('');
  const ticket=useRef(0),active=useRef(props.active!==false);
  useLayoutEffect(()=>owner.subscribe(()=>render(value=>value+1)),[owner]);
  useLayoutEffect(()=>{active.current=props.active!==false;ticket.current++;setPending(null);setConfirmTask(null);setBusy(false);return()=>{active.current=false;ticket.current++;};},[props.active,props.route,props.scopeIdentity,owner]);
  useLayoutEffect(()=>{try{owner.setScope(props.scopeIdentity);}catch{}},[owner,props.scopeIdentity]);
  const {studio,planner,proof}=owner.view(),strategy=studio.strategy,plan=planner.plan;
  const edit=(update:Parameters<StrategyWorkspace['editStudio']>[0])=>{setError('');owner.editStudio(update);};
  const act=(work:()=>void|Promise<void>)=>{setError('');void Promise.resolve().then(work).catch(error=>setError(message(error)));};
  const choose=(value:Strategy,origin?:string)=>{
    const revision=studio.revision??0;
    const apply=()=>{if(owner.view().studio.revision!==revision)throw Error('The Studio draft changed. Choose the template again.');edit(prior=>({strategy:value,origin,params:Object.fromEntries(value.params.map(param=>[param.name,param.default??''])),pathPrefix:'',objective:prior.objective,profile:prior.profile,promptEdited:false}));props.onNavigate?.('strategies/studio');};
    if(studioHasWork(studio))setPending({title:'Replace the current Studio draft?',revision,kind:'studio',apply});else apply();
  };
  const importFile=async(file:File|undefined,kind:'studio'|'plan')=>{
    if(!file)return;const revision=kind==='studio'?(studio.revision??0):planner.revision,original=ticket.current;
    if(file.size>(kind==='studio'?STUDIO_LIMIT_BYTES:262144))throw Error('The selected file exceeds the existing import budget.');
    const text=await file.text();owner.check();if(ticket.current!==original||!active.current)throw Error('The view changed during import; no draft was replaced.');
    let apply:()=>void,title:string;
    if(kind==='plan') {const imported=parsePlanFile(text);title='Replace the current Planner draft?';apply=()=>{if(!owner.replacePlan(imported.plan,revision))throw Error('Planner changed while importing. Choose the file again.');setNotice(imported.warnings.join(' '));};}
    else {const raw:unknown=JSON.parse(text),document=validateStudio(raw),checked=validateStrategy(raw);if(!document&&!checked.ok)throw Error('Choose a validated AIRK Studio or Strategy JSON file.');title='Replace the current Studio draft?';apply=()=>{if((owner.view().studio.revision??0)!==revision)throw Error('Studio changed while importing. Choose the file again.');const value=document??{strategy:checked.ok?checked.strategy:undefined};if(!value.strategy)throw Error('Strategy missing.');edit(()=>({...('params'in value?value:{}),strategy:value.strategy,params:'params'in value?value.params:{},pathPrefix:'pathPrefix'in value?value.pathPrefix:'',objective:'objective'in value?value.objective:'',profile:'profile'in value?value.profile:'portable',promptEdited:!!document?.editedPrompt,prompt:document?.editedPrompt}));};}
    const navigate=()=>{apply();props.onNavigate?.(kind==='plan'?'strategies/planner':'strategies/studio');};
    if(kind==='plan'?!!planner.plan:studioHasWork(studio))setPending({title,revision,kind,apply:navigate});else navigate();
  };
  const run=async()=>{
    if(!strategy)return;const entry=CORE_PACK.find(entry=>entry.strategy.id===strategy.id);if(entry&&!entry.runnable)throw Error(entry.reason??'This template is preview only.');
    const capture=props.capture(),revision=studio.revision,original=++ticket.current;setBusy(true);setNotice('');
    try{const result=await runBrowserStrategy({strategy,params:studio.params,scope:studio.pathPrefix?{pathPrefix:studio.pathPrefix}:{}},capture,{checkCurrent:()=>{owner.check();if(ticket.current!==original||!active.current||owner.view().studio.revision!==revision)throw Error('Studio changed; the old run cannot update it.');}});if(!owner.acceptRun(result,revision))throw Error('The draft changed; the earlier run was not applied.');setNotice(`Run ${result.run.manifest.outcome}; ${result.reads.length} complete native read receipt(s) retained.`);}finally{if(ticket.current===original)setBusy(false);}
  };
  const prompt=studioPrompt(studio),checkedPlan=plan?validatePlan(plan):undefined;
  const markdown=plan&&checkedPlan?.ok?renderPlan(plan,{workspace:'Browser import/download; workspace path not verified',extensionVersion:browserProductVersion,currentPublication:studio.run?.manifest.publication}):'';
  const editTask=(id:string,update:(task:PlanTask)=>PlanTask)=>owner.editPlan(plan=>({...plan,tasks:plan.tasks.map(task=>task.id===id?update(task):task)}));
  return <section aria-label="Strategies workspace">
    {error&&<p role="alert">{error}</p>}{notice&&<p role="status">{notice}</p>}
    <p>Strategies assemble bounded indexed evidence and prompts. Nothing here executes an agent. Browser downloads do not confirm a save in <code>.airk/plans</code>; existing workspace save and overwrite guards remain in the editor.</p>
    {props.route==='strategies/library'&&<><h2>Strategy Library</h2><p>All 25 versioned templates use the shared library. Preview entries keep their original limitations.</p><label>Category<select value={category} onChange={event=>setCategory(event.currentTarget.value)}><option value="all">All categories</option>{CATEGORIES.map(name=><option value={name}>{name}</option>)}</select></label><label>Filter<input value={filter} maxLength={120} onInput={event=>setFilter(event.currentTarget.value)}/></label><div class="grid">{CORE_PACK.filter(entry=>(category==='all'||entry.strategy.category===category)&&`${entry.strategy.title} ${entry.strategy.summary}`.toLowerCase().includes(filter.trim().toLowerCase())).map(entry=><article class="card" key={entry.strategy.id}><h3>{entry.strategy.title}</h3><p><code>{entry.strategy.id}@{entry.strategy.version}</code> · {entry.strategy.category}</p><p>{entry.strategy.summary}</p><p>{entry.runnable?'Runnable':`Preview: ${entry.reason}`}</p><ol>{entry.strategy.steps.map(step=><li>{operator(step.op)?.title??step.op} · <code>{step.op}</code></li>)}</ol><button onClick={()=>act(()=>choose(entry.strategy,entry.strategy.id))}>Load in Studio</button></article>)}</div><label>Import Strategy or Studio JSON<input type="file" accept=".json,application/json" onChange={event=>{const file=event.currentTarget.files?.[0];event.currentTarget.value='';act(()=>importFile(file,'studio'));}}/></label></>}
    {props.route==='strategies/studio'&&<><h2>Strategy Studio</h2>{!strategy?<p>Choose a Library template or import a Strategy or Studio file.</p>:<>
      <h3>{strategy.title}</h3><label>Objective<textarea value={studio.objective} maxLength={4000} onInput={event=>edit(draft=>({...draft,objective:event.currentTarget.value}))}/></label>
      {strategy.params.map(param=><label key={param.name}>{param.label}<input value={studio.params[param.name]??param.default??''} maxLength={512} onInput={event=>{const value=event.currentTarget.value;edit(draft=>({...draft,params:{...draft.params,[param.name]:value}}));}}/></label>)}
      <label>Relative path prefix<input value={studio.pathPrefix} maxLength={1024} onInput={event=>edit(draft=>({...draft,pathPrefix:event.currentTarget.value}))}/></label>
      <label>Prompt profile<select value={studio.profile} onChange={event=>{const profile=event.currentTarget.value as 'portable'|'mcp_aware';edit(draft=>({...draft,profile,prompt:draft.promptEdited?draft.prompt:undefined}));}}><option value="portable">Portable</option><option value="mcp_aware">MCP aware</option></select></label>
      <p>Budget: {strategy.budgets.calls} calls, {strategy.budgets.seconds} seconds, {strategy.budgets.evidenceBytes} evidence bytes, fanout {strategy.budgets.fanout}. Saved-byte receipts are native observations; the browser cannot inspect unsaved editor buffers.</p>
      <button disabled={busy||props.disabled} onClick={()=>act(run)}>{busy?'Running…':'Assemble evidence'}</button><button onClick={()=>act(()=>{edit(draft=>({...draft,prompt:compilePrompt({strategy,objective:draft.objective,profile:draft.profile,run:draft.run}),promptEdited:false,promptStale:false}));})}>Rebuild prompt</button>
      <label>Compiled prompt<textarea value={prompt} maxLength={262144} onInput={event=>edit(draft=>({...draft,prompt:event.currentTarget.value,promptEdited:true}))}/></label>{studio.promptStale&&<p role="status">The edited prompt is stale. Rebuild or review it before adding a Planner snapshot.</p>}
      <button onClick={()=>act(()=>copy(prompt))}>Copy prompt</button><button onClick={()=>download('strategy-prompt.md',prompt,'text/markdown')}>Download prompt</button>
      <button onClick={()=>act(()=>{const value=studioDocument(studio);if(!validateStudio(value))throw Error('Studio inputs are invalid; review their path and parameters.');download('airk-studio.json',JSON.stringify(value,null,2));})}>Download Studio JSON</button>
      <button disabled={!canAddStudio(studio)||(plan?.tasks.length??0)>=40} onClick={()=>act(()=>owner.editPlan(plan=>{if(plan.tasks.length>=40)throw Error('Planner already has 40 tasks.');return {...plan,tasks:[...plan.tasks,taskFromStudio(plan,studio)]};}))}>Add snapshot to Planner</button>
      {proof&&<><p>Outcome: {proof.run.manifest.outcome}; omitted evidence records: {proof.run.omitted}. {proof.run.manifest.reasons.join(' · ')}</p><button onClick={()=>download('airk-strategy-run.json',JSON.stringify(proof,null,2))}>Download complete run and native receipts</button><details><summary>Complete run and native receipts</summary><pre>{JSON.stringify(proof,null,2)}</pre></details></>}
      <details><summary>Strategy source and validation</summary><button onClick={()=>setJson(JSON.stringify(strategy,null,2))}>Edit a copy as JSON</button><textarea aria-label="Strategy JSON" value={json} maxLength={STRATEGY_LIMIT_BYTES} onInput={event=>setJson(event.currentTarget.value)}/><button onClick={()=>act(()=>{const checked=validateStrategy(JSON.parse(json));if(!checked.ok)throw Error(checked.errors.join('; '));choose(checked.strategy);})}>Validate and load Strategy</button></details>
    </>}</>}
    {props.route==='strategies/planner'&&<><h2>Planner</h2><button onClick={()=>{if(plan)setPending({title:'Replace the current Planner draft?',kind:'plan',revision:planner.revision,apply:()=>{if(!owner.replacePlan({schema:'airk.explorer.plan/1',title:'New plan',objective:studio.objective,tasks:[]},planner.revision))throw Error('Planner changed.');}});else owner.editPlan(value=>value);}}>New plan</button><label>Import Plan JSON or AIRK Markdown<input type="file" accept=".json,.md,application/json,text/markdown" onChange={event=>{const file=event.currentTarget.files?.[0];event.currentTarget.value='';act(()=>importFile(file,'plan'));}}/></label>
      {plan&&<><label>Plan title<input value={plan.title} maxLength={160} onInput={event=>owner.editPlan(plan=>({...plan,title:event.currentTarget.value}))}/></label><label>Plan objective<textarea value={plan.objective} maxLength={4000} onInput={event=>owner.editPlan(plan=>({...plan,objective:event.currentTarget.value}))}/></label>
      <button disabled={plan.tasks.length>=40} onClick={()=>owner.editPlan(plan=>({...plan,tasks:[...plan.tasks,{id:nextId(plan),title:'New task',instructions:'',dependsOn:[],state:'draft'}]}))}>Add manual task</button>
      {plan.tasks.map(task=><article class="card" key={task.id}><h3>{task.id} · {effectiveState(plan,task,studio.run?.manifest.publication)}</h3><label>Task title<input value={task.title} maxLength={160} onInput={event=>editTask(task.id,task=>({...task,title:event.currentTarget.value}))}/></label><label>Instructions<textarea value={task.instructions} maxLength={8000} onInput={event=>editTask(task.id,task=>({...task,instructions:event.currentTarget.value}))}/></label><label>Dependencies, comma separated<input value={task.dependsOn.join(',')} onChange={event=>{const dependsOn=event.currentTarget.value.split(',').map(id=>id.trim()).filter(Boolean);editTask(task.id,task=>({...task,dependsOn}));}}/></label><label>Stored state<select value={task.state} onChange={event=>{const state=event.currentTarget.value as StoredTaskState;if(state==='completed')setConfirmTask({id:task.id,revision:planner.revision});else editTask(task.id,task=>({...task,state}));}}>{TASK_STATES.map(state=><option value={state}>{state}</option>)}</select></label><button onClick={()=>act(()=>copy(task.instructions))}>Copy task instructions</button></article>)}
      {checkedPlan&&!checkedPlan.ok&&<p role="alert">{checkedPlan.errors.join('; ')}</p>}<button disabled={!checkedPlan?.ok} onClick={()=>download('airk-plan.json',JSON.stringify(plan,null,2))}>Download exact Plan JSON</button><button disabled={!checkedPlan?.ok} onClick={()=>download('airk-plan.md',markdown,'text/markdown')}>Download Plan Markdown</button><details><summary>Plan Markdown</summary><pre>{markdown}</pre></details></>}
    </>}
    {pending&&<div role="group" data-strategy-confirmation aria-label={pending.title}><p>{pending.title} Unsaved edits will be replaced.</p><button onClick={()=>act(()=>{const value=pending;setPending(null);value.apply();})}>Replace draft</button><button onClick={()=>setPending(null)}>Keep current draft</button></div>}
    {confirmTask&&<div role="group" data-strategy-confirmation aria-label="Confirm task completion"><p>Confirm that this task was reviewed and its completion gates were met. This does not execute or verify any agent work.</p><button onClick={()=>act(()=>{const confirmation=confirmTask;setConfirmTask(null);if(owner.view().planner.revision!==confirmation.revision)throw Error('Planner changed while confirmation was open. Review the task again.');editTask(confirmation.id,task=>({...task,state:'completed'}));})}>Mark completed</button><button onClick={()=>setConfirmTask(null)}>Keep current state</button></div>}
  </section>;
}
