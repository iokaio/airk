// SPDX-License-Identifier: Apache-2.0
// Explicit browser change inputs over the same bounded Review algorithm as the editor.
import { READ_TRANSPORT_DEADLINE_MS } from '../../vscode/src/generated/arbiterBudget';
import {useEffect,useRef,useState} from 'preact/hooks';
import {createReviewOperations,DEFAULT_TEST_PATTERN,type ReviewPage,type ChangeKind} from '../../ui/src/review';
import {FactHandles} from '../../vscode/src/explorerFacts';
import {RefRegistry} from '../../vscode/src/explorerSearch';
import {EvidenceFailure,type EvidenceReadPort} from '../../ui/src/evidenceCore';
import {browserSha256,capturedSingleIndexResult,boundedWorkflowRead} from './strategyRunner';
import type {ScopedReadCapture,ScopedReadObservation} from './SearchScreen';
import {manualReviewChanges} from './browserReview';
import {completeEvidenceMarkdown} from './evidenceExports';
import {Text} from './render';
export const BROWSER_REVIEW_ROUTES=['review/changes','review/impact','review/tests'] as const;
export type BrowserReviewRoute=typeof BROWSER_REVIEW_ROUTES[number];
export const isBrowserReviewRoute=(v:unknown):v is BrowserReviewRoute=>typeof v==='string'&&(BROWSER_REVIEW_ROUTES as readonly string[]).includes(v);
type Rec=Record<string,unknown>;
type Report={scope:string;input:string;format:'paths'|'patch';kind:ChangeKind;result:unknown;reads:ScopedReadObservation[];pendingReads:number;historical?:boolean};
const names:Record<BrowserReviewRoute,string>={'review/changes':'Supplied changes','review/impact':'Impact candidates','review/tests':'Test candidates'};
const object=(v:unknown):Rec=>v!==null&&typeof v==='object'&&!Array.isArray(v)?v as Rec:{};
export function BrowserReviewScreen(props:{route:BrowserReviewRoute;scopeIdentity?:string;disabled:boolean;capture():ScopedReadCapture;active?:boolean}){
 const [text,setText]=useState(''),[format,setFormat]=useState<'paths'|'patch'>('paths'),[kind,setKind]=useState<ChangeKind>('modified'),[busy,setBusy]=useState(false),[error,setError]=useState(''),[reports,setReports]=useState<Partial<Record<BrowserReviewRoute,Report>>>({});
 const generation=useRef(0),active=useRef(props.active!==false),presentation=useRef({route:props.route,active:props.active!==false,epoch:0});active.current=props.active!==false;if(presentation.current.route!==props.route||presentation.current.active!==active.current)presentation.current={route:props.route,active:active.current,epoch:presentation.current.epoch+1};
 useEffect(()=>{if(props.active===false)generation.current++;},[props.active]);useEffect(()=>()=>{generation.current++;},[]);
 const report=reports[props.route],historical=!!report&&(report.scope!==props.scopeIdentity||report.input!==text||report.format!==format||report.kind!==kind||report.historical===true);
 const run=async(page?:ReviewPage)=>{if(busy||props.disabled)return;setBusy(true);setError('');const owner=++generation.current,viewEpoch=presentation.current.epoch,route=props.route,input=text,sourceFormat=format,sourceKind=kind,reads=page&&report?[...report.reads]:[];let pendingReads=0;
  try{
   const changes=manualReviewChanges(input,sourceFormat,sourceKind),capture=props.capture();
   if(capture.indexIds.length!==1||!capture.backingBinding)throw Error('Review needs one explicit originating index with native backing metadata. Narrow the named scope above and apply it.');
   const current=()=>{try{capture.checkCurrent();if(!active.current||generation.current!==owner||presentation.current.epoch!==viewEpoch||presentation.current.route!==route)throw Error('The Review view changed.');}catch{throw new EvidenceFailure({code:'STALE_BINDING',message:'The captured Review scope or input was replaced; no further reads were sent.'});}};
   const port:EvidenceReadPort={call:async(tool,args,options)=>{current();pendingReads++;const original=capture.read(tool.replace(/^airk_/,''),args);original.then(()=>{pendingReads--;},()=>{pendingReads--;});const reply=await boundedWorkflowRead(original,options?.timeout??READ_TRANSPORT_DEADLINE_MS).catch(failure=>{current();throw failure;});current();reads.push(reply);return capturedSingleIndexResult(capture,reply);}};
   const operations=createReviewOperations({sourceKind:'manual',changedPaths:async()=>({git:'unavailable',changes}),dirty:()=>false,testPattern:()=>new RegExp(DEFAULT_TEST_PATTERN),sha256:browserSha256});
   const context={session:port,handles:new FactHandles(new RefRegistry()),project:capture.backingBinding,root:'explicit-browser-input'};
   const result=route==='review/changes'?await operations.reviewChanges(context,page):route==='review/impact'?await operations.reviewImpact(context):await operations.reviewTests(context);
   current();setReports(prior=>({...prior,[route]:{scope:props.scopeIdentity!,input,format:sourceFormat,kind:sourceKind,result,reads,pendingReads}}));
  }catch(failure){const stillCurrent=active.current&&generation.current===owner&&presentation.current.epoch===viewEpoch&&presentation.current.route===route;if(stillCurrent)setError(failure instanceof Error?failure.message:String(failure));if(reads.length||pendingReads)setReports(prior=>({...prior,[route]:{scope:props.scopeIdentity??'',input,format:sourceFormat,kind:sourceKind,result:{analysis:'unavailable',error:failure instanceof Error?failure.message:String(failure)},reads,pendingReads,...(!stillCurrent?{historical:true}:{})}}));}
  finally{setBusy(false);}
 };
 const copy=async(value:string)=>{try{await navigator.clipboard.writeText(value);}catch(failure){setError(`Clipboard failed; select or export the complete retained result. ${String(failure)}`);}};
 const save=(markdown=false)=>{if(!report)return;try{const value=markdown?completeEvidenceMarkdown(report,{input:report.input,format:report.format,kind:report.kind}):JSON.stringify(report,null,2),url=URL.createObjectURL(new Blob([value],{type:markdown?'text/markdown':'application/json'})),a=document.createElement('a');a.href=url;a.download=markdown?'airk-review.md':'airk-review.json';a.click();setTimeout(()=>URL.revokeObjectURL(url),0);}catch(failure){setError(`Export failed; the complete result remains below. ${String(failure)}`);}};
 const body=object(report?.result),rows=Array.isArray(body.files)?body.files:Array.isArray(body.items)?body.items:Array.isArray(body.candidates)?body.candidates:[];
 return <section aria-label="Browser Review"><h2>{names[props.route]}</h2><p>The browser cannot inspect your Git repository, staged/worktree state or unsaved editor buffers. Supply exact relative changed paths or a diff. Native reads inspect only the currently selected published index. Impact and test rows are bounded textual candidates, never executed tests or complete semantic impact.</p>
  <form onSubmit={event=>{event.preventDefault();void run();}}><label>Input format<select disabled={busy} value={format} onChange={e=>setFormat(e.currentTarget.value as 'paths'|'patch')}><option value="paths">One exact relative indexed path per line</option><option value="patch">Git unified diff sections (binary, standalone and quoted paths require Paths mode)</option></select></label>
   {format==='paths'&&<label>Supplied change kind<select disabled={busy} value={kind} onChange={e=>setKind(e.currentTarget.value as ChangeKind)}>{['added','modified','deleted','renamed','untracked','conflicted'].map(value=><option value={value}>{value}</option>)}</select></label>}
   <label>Changed paths or patch<textarea disabled={busy} value={text} maxLength={65536} rows={8} onInput={e=>setText(e.currentTarget.value)}/></label><button disabled={busy||props.disabled||!text.trim()}>{busy?'Reading…':`Review ${names[props.route]}`}</button></form>
  {error&&<p role="alert"><Text value={error}/></p>}
  {report&&<section>{report.pendingReads>0&&<p role="alert">A native read may still be running after the original wait ended. No reply, completed work or replay is inferred.</p>}<p>Input source: explicitly supplied by the user. Git and unsaved-buffer observation: unavailable.</p>{historical&&<p role="alert">Historical scope or input. Run the current review before continuing this report.</p>}
   {body.analysis!==undefined&&<p>Candidate analysis: <Text value={String(body.analysis)}/></p>}{typeof body.analysisError==='string'&&<p role="alert"><Text value={body.analysisError}/></p>}{Number.isSafeInteger(body.total)&&<p>{rows.length} supplied paths shown of {String(body.total)} · offset {String(body.offset??0)}</p>}{Array.isArray(body.notes)&&body.notes.map(note=><p><Text value={String(note)}/></p>)}{body.truncated===true&&<p role="alert">This review is bounded or partial. Omitted candidates are not evidence of no impact or no tests.</p>}
   {rows.map(row=>{const value=object(row);return <article><h3><Text value={String(value.path??value.name??'Candidate')}/></h3><pre><Text value={JSON.stringify(value,null,2)}/></pre><button onClick={()=>void copy(JSON.stringify(value,null,2))}>Copy complete candidate</button></article>;})}
   {props.route==='review/changes'&&Number.isSafeInteger(body.offset)&&Number.isSafeInteger(body.limit)&&<div><button disabled={busy||props.disabled||historical||Number(body.offset)===0} onClick={()=>void run({offset:Math.max(0,Number(body.offset)-Number(body.limit)),snapshot:String(body.snapshot),publication:typeof body.publication==='string'?body.publication:undefined})}>Previous supplied paths</button><button disabled={busy||props.disabled||historical||body.truncated!==true} onClick={()=>void run({offset:Number(body.offset)+Number(body.limit),snapshot:String(body.snapshot),publication:typeof body.publication==='string'?body.publication:undefined})}>Next supplied paths</button></div>}
   <button onClick={()=>void copy(JSON.stringify(report,null,2))}>Copy complete Review JSON</button><button onClick={()=>void copy(completeEvidenceMarkdown(report,{input:report.input,format:report.format}))}>Copy complete Review Markdown</button><button onClick={()=>save(false)}>Export complete Review JSON</button><button onClick={()=>save(true)}>Export complete Review Markdown</button><details><summary>Complete Review result, original inputs and all native read receipts</summary><pre><Text value={JSON.stringify(report,null,2)}/></pre></details>
  </section>}
 </section>;
}
