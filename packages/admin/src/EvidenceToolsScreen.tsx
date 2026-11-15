// SPDX-License-Identifier: Apache-2.0
// Browser DOM adapter over retained guided inputs and owning fact/citation/native-source projections.
import {useEffect,useRef,useState} from 'preact/hooks';
import type {JSX} from 'preact';
import {Field,NumberField,Text} from './render';
import type {ScopedReadCapture,ScopedReadObservation} from './SearchScreen';
import {guidedEvidenceRequest,guidedIndexedFiles,type GuidedEvidenceRoute,type GuidedFields} from './guidedEvidence';
import {FactHandles,factList,continueFacts,registerCursor,continueNativePage} from '../../vscode/src/explorerFacts';
import {RefRegistry,originArgs} from '../../vscode/src/explorerSearch';
import type {FactListView} from '../../vscode/src/explorerContracts';
import {parseCitations} from '../../vscode/src/explorerContracts';
import type {NativeTool} from '../../ui/src/evidenceCore';
import {nativeSavedSourceState,verificationLabel,readResultBody} from '../../vscode/src/sourceVerification';
import {completeEvidenceMarkdown} from './evidenceExports';
import {publicationOf,absentFromIndex} from '../../ui/src/evidenceCore';

type Rec=Record<string,unknown>;
const rec=(v:unknown):Rec=>v!==null&&typeof v==='object'&&!Array.isArray(v)?v as Rec:{};
const names:Record<GuidedEvidenceRoute,string>={'code/symbols':'Symbols','code/references':'Textual references','code/outline':'Selected-source outline','docs/facts':'Document facts','evidence/inspector':'Inspector','evidence/checker':'Check citations','evidence/changes':'Indexed changes'};
const kinds=['any','requirement','decision','design','architecture','feature','acceptance','date','section','table_row','table_cell'];
export const guidedEvidenceTitle=(route:GuidedEvidenceRoute)=>names[route];
export interface GuidedInputSnapshot {route:GuidedEvidenceRoute;fields:GuidedFields;additionalArguments:Rec}
export interface GuidedToolReport {inputs:GuidedInputSnapshot;scope:string;reads:ScopedReadObservation[];facts?:FactListView;related?:FactListView[];changeMore?:string;checks?:{citation:string;state:string;message:string}[];error?:string;historical?:boolean}
function download(name:string,text:string,type='application/json'){const url=URL.createObjectURL(new Blob([text],{type})),a=document.createElement('a');a.href=url;a.download=name;a.click();setTimeout(()=>URL.revokeObjectURL(url),0);}
export function EvidenceToolsScreen(props:{route:GuidedEvidenceRoute;scopeIdentity?:string;disabled:boolean;capture():ScopedReadCapture;active?:boolean}):JSX.Element {
 const [fields,setFields]=useState<GuidedFields>({name:'',path:'',prefix:'',kind:'requirement',since:'',exact:true,limit:props.route==='code/outline'?100:50,text:''}),[extra,setExtra]=useState(''),[busy,setBusy]=useState(false),[error,setError]=useState(''),[report,setReport]=useState<GuidedToolReport>(),[paths,setPaths]=useState<string[]>([]),[direction,setDirection]=useState<'incoming'|'outgoing'>('outgoing'),[hops,setHops]=useState(1);
 const handles=useRef(new FactHandles(new RefRegistry()));
 const lifetime=useRef({active:props.active!==false,route:props.route,epoch:0});if(lifetime.current.active!==(props.active!==false)||lifetime.current.route!==props.route){lifetime.current={active:props.active!==false,route:props.route,epoch:lifetime.current.epoch+1};}
 useEffect(()=>()=>{lifetime.current.active=false;lifetime.current.epoch++;},[]);
 const put=<K extends keyof GuidedFields>(key:K,value:GuidedFields[K])=>setFields(prior=>({...prior,[key]:value}));
 const advanced=():Rec=>{const value=extra.trim()?JSON.parse(extra):{};if(value===null||Array.isArray(value)||typeof value!=='object')throw Error('Additional native arguments must be a JSON object.');return value;};
 const project=props.scopeIdentity??'',stale=!!report&&(report.scope!==project||report.historical===true);
 const run=async(action?:{tool:string;args:Rec},files=false)=>{
  if(busy||props.disabled||!lifetime.current.active)return;const viewEpoch=lifetime.current.epoch;const viewCurrent=()=>{if(!lifetime.current.active||lifetime.current.epoch!==viewEpoch)throw Error('The original guided view was hidden or replaced. No further reads were sent.');};setBusy(true);setError('');const previous=action&&report?.scope===project&&!stale?report:undefined;const reads:ScopedReadObservation[]=previous?[...previous.reads]:[];let capture:ScopedReadCapture|undefined,inputs:GuidedInputSnapshot|undefined;
  try{
   const supplied=advanced();inputs=previous?.inputs??{route:props.route,fields:structuredClone(fields),additionalArguments:structuredClone(supplied)};
   viewCurrent();capture=props.capture();capture.checkCurrent();const scope=project;
   const read=async(tool:string,args:Rec)=>{viewCurrent();capture!.checkCurrent();const result=await capture!.read(tool,args);viewCurrent();capture!.checkCurrent();if(result.scopeIdentity!==scope)throw Error('The displayed scope was replaced before this result returned.');reads.push(result);return result;};
   if(props.route==='evidence/checker'&&!action){
    if(capture.indexIds.length!==1)throw Error('Checking pasted paths needs one explicitly chosen index. Narrow the named sources above and apply before checking.');
    const parsed=parseCitations(fields.text),unique=[...new Set(parsed.citations.filter(c=>!c.absolute).map(c=>c.path))],byPath=new Map<string,Rec>();
    if(!parsed.citations.length)throw Error('No supported relative path:line citations were found. This is not a successful check.');
    for(const path of unique.slice(0,25)){const result=await read('file_info',{...supplied,path});const members=Array.isArray(result.logical.members)?result.logical.members.map(rec):[];if(members.length!==1||members[0].index_id!==capture.indexIds[0])throw Error('The returned member does not identify the selected index.');const body=readResultBody(members[0].result);if(!Object.keys(body).length)throw Error('The canonical member file record could not be read; the complete reply remains available.');byPath.set(path,body);}
    const checks=parsed.citations.map(c=>{const info=byPath.get(c.path),lines=info?.indexed_lines;let state='unchecked',message='No matching native source receipt was observed.';if(c.absolute){state='outside_collection';message='Absolute and parent-relative paths are not checked.';}else if(!info){message='Not checked: more than25distinct paths in one check.';}else if(absentFromIndex(info)){state='not_found';message='Not present in this selected index publication.';}else if(Number.isSafeInteger(lines)&&c.endLine>Number(lines)){state='line_out_of_range';message=`Beyond ${lines} indexed lines.`;}else if(Number.isSafeInteger(lines)){const hash=info.content_hash;state=typeof hash==='string'?nativeSavedSourceState(info,hash,publicationOf(info)):'unchecked';message=verificationLabel(info.source_verification);}return {citation:c.text,state,message};});
    viewCurrent();capture.checkCurrent();setReport({inputs,scope,reads,checks,...((parsed.limited||unique.length>25)?{error:[...(parsed.limited?['Citation parser limit reached; further citations were not checked.']:[]),...(unique.length>25?[`Only25 of ${unique.length} distinct relative paths were checked; ${unique.length-25} were not checked.`]:[])].join(' ')}:{})});
   }else{
    const request=action??guidedEvidenceRequest(props.route,fields,supplied),result=await read(request.tool,request.args);
    const facts=['symbols','code_facts','references','outline','document_facts','list_files','relationships'].includes(request.tool)?factList({tool:('airk_'+request.tool) as NativeTool,args:request.args,limit:fields.limit},result.logical,publicationOf(result.logical),scope,handles.current,result.raw):undefined;
    if(files&&facts)setPaths([...new Set(facts.items.map(item=>item.path))]);
    const changeMore=request.tool==='changes'?registerCursor({tool:'airk_changes',args:request.args,limit:fields.limit},result.logical,publicationOf(result.logical),scope,handles.current):undefined;
    viewCurrent();capture.checkCurrent();setReport({...previous,inputs,scope,reads,...(request.tool==='changes'?{changeMore}:{}),...(previous?.facts?{facts:previous.facts,related:[...(previous.related??[]),...(facts?[facts]:[])]}:facts?{facts}:{})});
   }
  }catch(failure){const message=failure instanceof Error?failure.message:String(failure);setError(message);if(reads.length&&inputs)setReport({inputs,scope:project,reads,error:message});}
  finally{setBusy(false);}
 };
 const copy=async(text:string)=>{try{await navigator.clipboard.writeText(text);}catch(failure){setError(`Clipboard write failed. Select or export the complete retained result below. ${failure instanceof Error?failure.message:String(failure)}`);}};
 const save=(name:string,text:string,type?:string)=>{try{download(name,text,type);}catch(failure){setError(`Export failed; the complete result remains below. ${failure instanceof Error?failure.message:String(failure)}`);}};
 const more=async(shown:FactListView,position:number)=>{
  if(!report||!shown.more||busy||props.disabled||stale||!lifetime.current.active)return;const epoch=lifetime.current.epoch;const current=()=>{if(!lifetime.current.active||lifetime.current.epoch!==epoch)throw Error('The original guided view was hidden or replaced.');};setBusy(true);setError('');
  const reads=[...report.reads];try{const capture=props.capture(),port={call:async(tool:NativeTool,args:Rec)=>{current();capture.checkCurrent();const result=await capture.read(tool,args);current();capture.checkCurrent();reads.push(result);return {payload:result.logical,rawPayload:result.raw,publication:publicationOf(result.logical)};}};
   const next=await continueFacts(port,handles.current,report.scope,shown.more);current();capture.checkCurrent();const updated={...next,items:[...shown.items,...next.items],returned:shown.items.length+next.items.length};setReport({...report,reads,...(position===0?{facts:updated}:{related:report.related?.map((item,index)=>index===position-1?updated:item)})});
  }catch(failure){setError(failure instanceof Error?failure.message:String(failure));setReport({...report,reads,historical:true});}finally{setBusy(false);}
 };
 const fileRoute=props.route==='code/outline'||props.route==='evidence/inspector';
 const moreChanges=async()=>{
  if(!report?.changeMore||busy||props.disabled||stale||!lifetime.current.active)return;
  const epoch=lifetime.current.epoch,current=()=>{if(!lifetime.current.active||lifetime.current.epoch!==epoch)throw Error('The original guided view was hidden or replaced.');};
  const reads=[...report.reads];setBusy(true);setError('');
  try{const capture=props.capture(),port={call:async(tool:NativeTool,args:Rec)=>{current();capture.checkCurrent();const result=await capture.read(tool,args);current();capture.checkCurrent();if(result.scopeIdentity!==report.scope)throw Error('The displayed scope was replaced before this result returned.');reads.push(result);return {payload:result.logical,rawPayload:result.raw,publication:publicationOf(result.logical)};}};
   const {handle,result}=await continueNativePage(port,handles.current,report.scope,report.changeMore);
   const changeMore=registerCursor(handle,result.payload,result.publication,report.scope,handles.current);current();capture.checkCurrent();setReport({...report,reads,changeMore});
  }catch(failure){const message=failure instanceof Error?failure.message:String(failure);setError(message);setReport({...report,reads,error:message,historical:true});}finally{setBusy(false);}
 };
 return <section aria-label={names[props.route]}><h2>{names[props.route]}</h2>
  <p>{props.route==='code/references'?'Exact-case textual occurrences include comments, strings and definitions; they are not resolved references or calls.':props.route==='docs/facts'?'Extraction candidates retain original document locations and limits; they are not verified requirements.':props.route==='evidence/changes'?'Compare indexed publications. This is not Git changes or a claim about unsaved editor buffers.':props.route==='evidence/inspector'?'Read the indexed source record and its dated native saved-byte check. A browser cannot observe unsaved editor changes or open a VSCode file.':props.route==='evidence/checker'?'Check supported citations in pasted text against one selected indexed source, its line count and actual dated native source receipt.':'Read indexed code facts with provenance and coverage. Written relationships remain candidates.'}</p>
  <form onSubmit={event=>{event.preventDefault();void run();}}>
   {(props.route==='code/symbols'||props.route==='code/references'||props.route==='docs/facts')&&<Field label={props.route==='docs/facts'?'Text contains':'Identifier or symbol name'} value={fields.name} onInput={value=>put('name',value)}/>}
   {props.route==='code/symbols'&&<label><input type="checkbox" checked={fields.exact} onChange={e=>put('exact',e.currentTarget.checked)}/>Code facts with exact names and symbol IDs</label>}
   {fileRoute&&<><Field label="Indexed relative source path" value={fields.path} onInput={value=>put('path',value)}/><button type="button" disabled={busy||props.disabled} onClick={()=>void run(guidedIndexedFiles(fields.prefix),true)}>Choose from indexed files</button>{paths.length>0&&<label>Indexed files<select value={fields.path} onChange={e=>put('path',e.currentTarget.value)}><option value="">Choose a source</option>{paths.map(path=><option value={path}>{path}</option>)}</select></label>}</>}
   {props.route!=='evidence/checker'&&<Field label="Path prefix (optional)" value={fields.prefix} onInput={value=>put('prefix',value)}/>}
   {props.route==='docs/facts'&&<label>Fact kind<select value={fields.kind} onChange={e=>put('kind',e.currentTarget.value)}>{!kinds.includes(fields.kind)&&<option value={fields.kind}>{fields.kind}</option>}{kinds.map(kind=><option value={kind}>{kind}</option>)}</select></label>}
   {props.route==='docs/facts'&&<Field label="Other indexed fact kind (optional)" value={kinds.includes(fields.kind)?'':fields.kind} onInput={value=>put('kind',value||'any')}/>}
   {props.route==='evidence/changes'&&<Field label="Earlier native publication token" value={fields.since} onInput={value=>put('since',value)}/>}
   {props.route==='evidence/checker'&&<label>Report or citations<textarea value={fields.text} maxLength={65536} rows={8} onInput={e=>put('text',e.currentTarget.value)}/></label>}
   {props.route!=='evidence/inspector'&&props.route!=='evidence/checker'&&<NumberField label="Result limit" value={fields.limit} min={1} max={100} onInput={value=>put('limit',value)}/>}
   <details><summary>Additional supported native arguments</summary><p>Every additional field is retained. Visible nonempty inputs and the shown bounds take precedence. Native validation and the current management scope remain authoritative.</p><textarea rows={5} value={extra} onInput={e=>setExtra(e.currentTarget.value)}/></details>
   <button type="submit" disabled={busy||props.disabled}>{busy?'Reading…':props.route==='evidence/checker'?'Check citations':`Read ${names[props.route]}`}</button>
  </form>
  {error&&<p role="alert"><Text value={error}/></p>}
  {report&&<section aria-label="Guided evidence results">{stale&&<p role="alert">Historical result from another scope. Reapply the intended sources and run again; source actions are disabled.</p>}
   {[...(report.facts?[report.facts]:[]),...(report.related??[])].map((shownFacts,position)=><section aria-label={`Fact result ${position+1}`}><div><label>Relationship direction<select value={direction} onChange={e=>setDirection(e.currentTarget.value as 'incoming'|'outgoing')}><option value="outgoing">Outgoing candidates</option><option value="incoming">Incoming candidates</option></select></label><NumberField label="Relationship hops" value={hops} min={1} max={3} onInput={setHops}/></div><p>{shownFacts.returned} returned · {shownFacts.truncated?'bounded or partial':'native page'} · {shownFacts.notes.join(' ')}</p>{shownFacts.items.map(item=><article><h3><Text value={item.name||item.path}/></h3><p><Text value={`${item.path}:${item.startLine??''}–${item.endLine??''} ${item.location??''}`}/></p><p><Text value={item.signature??item.detail??item.kind}/></p>{item.ref&&<button disabled={busy||props.disabled||stale} onClick={()=>{const ref=handles.current.refs.get(item.ref!,report.scope);void run({tool:'file_info',args:{path:ref.path,...originArgs(ref.origin)}});}}>Inspect indexed source</button>}{item.symbolRef&&<button disabled={busy||props.disabled||stale} onClick={()=>{const symbol=handles.current.symbols.get(item.symbolRef!,report.scope);void run({tool:'relationships',args:{symbol:symbol.symbolId,direction,hops,limit:50,max_bytes:16384,...originArgs(symbol.origin)}});}}>Show relationship candidates</button>}<button onClick={()=>void copy(`${item.path}:${item.startLine??1}${item.endLine?'–'+item.endLine:''}`)}>Copy source location</button><details><summary>Complete projected fact and citation</summary><pre><Text value={JSON.stringify(item,null,2)}/></pre></details></article>)}{shownFacts.more&&<button disabled={busy||props.disabled||stale} onClick={()=>void more(shownFacts,position)}>Load next native page</button>}</section>)}
   {report.reads.map((read,readNumber)=>{
    const members=Array.isArray(read.logical.members)?read.logical.members.map(rec):[{result:read.logical}];
    return members.map((member,memberNumber)=>{const body=readResultBody(member.result);return <section aria-label={`Native source observations ${readNumber+1}.${memberNumber+1}`}><h3><Text value={String(member.name??member.index_id??'Selected source')}/></h3>{member.outcome&&<p><Text value={String(member.outcome)}/></p>}
     {['content_hash','indexed_lines','source_bytes','parse_status','language','line_basis','extraction_method','publication','source_verification','legacy_saved_evidence','baseline_status','status','coverage','truncated','error'].map(key=>body[key]!==undefined?<p><strong>{key}: </strong><Text value={JSON.stringify(body[key])}/></p>:null)}
     {Array.isArray(body.changes)&&body.changes.map(change=><article><h4>Indexed change</h4><pre><Text value={JSON.stringify(change,null,2)}/></pre></article>)}
    </section>;});
   })}
   {report.checks?.map(check=><article><strong><Text value={check.citation}/></strong><p><Text value={`${check.state}: ${check.message}`}/></p></article>)}
   {report.changeMore&&<p role="status">More indexed changes remain in the native comparison. <button disabled={busy||props.disabled||stale} onClick={()=>void moreChanges()}>Load more indexed changes</button></p>}
   {report.error&&<p role="alert"><Text value={report.error}/></p>}<button onClick={()=>void copy(JSON.stringify(report,null,2))}>Copy complete guided run JSON</button><button onClick={()=>save('airk-guided-evidence.json',JSON.stringify(report,null,2))}>Export complete guided run JSON</button><button onClick={()=>void copy(completeEvidenceMarkdown(report,report.reads.map(read=>read.request)))}>Copy complete guided run Markdown</button><button onClick={()=>save('airk-guided-evidence.md',completeEvidenceMarkdown(report,report.reads.map(read=>read.request)),'text/markdown')}>Export complete guided run Markdown</button>
   {report.reads.map((read,index)=><details><summary>Complete native read {index+1}: arguments, result and scope</summary><pre><Text value={JSON.stringify(read,null,2)}/></pre></details>)}
  </section>}
 </section>;
}
