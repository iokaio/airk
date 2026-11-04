// SPDX-License-Identifier: Apache-2.0
import {nativeVerification,readResultBody,selectedVerification,verificationLabel} from './sourceVerification';
import { Continuation, EvidenceCard, object, Value, WorkbenchSession, WorkbenchTurn } from './workbenchContracts';

export function continuationCursor(raw:unknown):string|undefined {
    const result=object(raw),content=object(result.content),evidence=object(result.evidence);
    return [result.cursor,content.cursor,object(result.page).cursor,object(content.page).cursor,object(evidence.page).cursor]
        .find((value):value is string=>typeof value==='string'&&value.length>0);
}
export function continuationRequiredRecordBytes(raw:unknown):number|undefined {
    const bytes=resultMetadata(raw).required_record_bytes;
    return Number.isSafeInteger(bytes)&&Number(bytes)>0?Number(bytes):undefined;
}
export function continuations(envelope:Value):Continuation[]{
    const remaining=Number(object(object(envelope.plan).budgets).calls??12)-Number(envelope.calls??0);
    return (Array.isArray(envelope.steps)?envelope.steps:[]).flatMap((raw,i)=>{const report=object(raw),cursor=continuationCursor(report.result);
        const requiredRecordBytes=continuationRequiredRecordBytes(report.result);
        return typeof cursor==='string'&&report.status==='complete'&&remaining>=2?[{id:`page-${i}`,tool:String(report.tool),callsRemaining:remaining,...(requiredRecordBytes!==undefined?{requiredRecordBytes}:{})}]:[];
    });
}

export interface DiscoveryStepDetails {
    id:string;tool:string;canFollow:boolean;count?:Value;files:{path:string;count:number}[];filesTruncated:boolean;
    replyDetail?:'lean'|'rich';
    guidance?:Value;
    readGuidance?:Value;
    sourceVerification?:Value;
    execution?:Value;semanticReadiness?:Value;discoveryReadiness?:Value;readError?:Value;scopeObservation?:Value;
    requestedMode?:string;publication?:unknown;
    coverage?:Value;countUnit?:string;
    more:boolean;incompleteReason?:string;suggestions:{target:string;symbolId?:string;path?:string;coverage?:string}[];
    definitions:Value[];omittedFiles:number;omittedSuggestions:number;omittedDefinitions:number;
    requiredRecordBytes?:number;
}
function guidanceMetadata(raw:unknown):Value|undefined {
    const value=object(raw);if(value.version!==1||!Array.isArray(value.observations))return undefined;
    let truncated=value.truncated===true||value.observations.length>8;
    const observations=value.observations.slice(0,8).flatMap(raw=>{
        const row=object(raw),keys=['reason','subject','basis','message','next_read'];
        if(keys.some(key=>typeof row[key]!=='string'||String(row[key]).length>512)){truncated=true;return [];}
        return [Object.fromEntries(keys.map(key=>[key,row[key]]))];
    });
    return {version:1,observations,truncated};
}
function canFollowResult(envelope:Value,report:Value):boolean {
    const result=object(report.result),content=object(result.content),evidence=object(result.evidence);
    const publication=result.publication??content.publication??evidence.publication;
    const token=typeof publication==='string'?publication:object(publication).token;
    return envelope.executed===true&&['complete','partial'].includes(String(envelope.status))&&report.status==='complete'&&typeof envelope.publication==='string'&&envelope.publication.length>0&&token===envelope.publication;
}
function resultMetadata(raw:unknown):Value {
    const result=readResultBody(raw),content=object(result.content),evidence=object(result.evidence);
    const page=Object.keys(object(result.page)).length?object(result.page):Object.keys(object(content.page)).length?object(content.page):object(evidence.page);
    const source={...result,...content};
    return Object.fromEntries(Object.entries({
        cursor:continuationCursor(raw),truncated:source.truncated??evidence.truncated,coverage:page.coverage??source.coverage??evidence.coverage,resolution:source.resolution??evidence.resolution,
        match_count:source.match_count??page.match_count,files:page.files??source.files,
        files_truncated:page.files_truncated??source.files_truncated,more:page.more,
        incomplete_reason:page.incomplete_reason,required_record_bytes:page.required_record_bytes??source.required_record_bytes,
        count_unit:page.count_unit,definitions:source.definitions,suggested_targets:source.suggested_targets,
        reply_detail:['lean','rich'].includes(String(source.reply_detail))?source.reply_detail:undefined,
        guidance:guidanceMetadata(source.guidance??evidence.guidance),
        read_guidance:Object.keys(object(source.read_guidance)).length?source.read_guidance:undefined,
        source_verification:nativeVerification(source.source_verification),
        execution:source.execution??evidence.execution,
        semantic_readiness:source.semantic_readiness??evidence.semantic_readiness,
        discovery_readiness:source.discovery_readiness??evidence.discovery_readiness,
        error:source.error??evidence.error,requested_mode:source.requested_mode??evidence.requested_mode,
        publication:source.publication??evidence.publication,
        scope_observation:source.scope_observation??evidence.scope_observation??object(raw).scope_observation,
    }).filter(([,value])=>value!==undefined));
}
/** Describe only the recognized execution schema; unknown metadata stays intact. */
export function searchExecutionLabel(raw:unknown):string|undefined {
    const value=object(raw),requested=value.requested_mode,served=value.served_mode;
    if(value.version!==1||typeof requested!=='string'||!['default','lexical','vector','hybrid'].includes(requested)
        ||typeof served!=='string'||!['lexical','vector','hybrid'].includes(served)||typeof value.incomplete!=='boolean')return undefined;
    return `Search requested ${requested}; served ${served} evidence.${value.incomplete?' The semantic component is incomplete. Saved-source verification applies to returned file references and does not establish semantic completeness.':''}`;
}
/** Bounded display projection only; exports below retain the native metadata itself. */
export function discoveryDetails(envelope:Value):DiscoveryStepDetails[]{
    return (Array.isArray(envelope.steps)?envelope.steps:[]).flatMap((raw,i)=>{
        const report=object(raw),metadata=resultMetadata(report.result);
        if(!Object.keys(metadata).length)return [];
        const files=Array.isArray(metadata.files)?metadata.files:[],suggestions=Array.isArray(metadata.suggested_targets)?metadata.suggested_targets:[],definitions=Array.isArray(metadata.definitions)?metadata.definitions:[];
        const count=object(metadata.match_count),coverage=object(metadata.coverage);
        return [{id:String(report.id??`step-${i}`),tool:String(report.tool??''),canFollow:canFollowResult(envelope,report),...(Object.keys(count).length?{count}:{}),
            ...(metadata.reply_detail==='lean'||metadata.reply_detail==='rich'?{replyDetail:metadata.reply_detail}:{}),
            ...(metadata.guidance?{guidance:object(metadata.guidance)}:{}),
            ...(metadata.read_guidance?{readGuidance:object(metadata.read_guidance)}:{}),
            ...(metadata.source_verification?{sourceVerification:object(metadata.source_verification)}:{}),
            ...(Object.keys(object(metadata.execution)).length?{execution:object(metadata.execution)}:{}),
            ...(Object.keys(object(metadata.semantic_readiness)).length?{semanticReadiness:object(metadata.semantic_readiness)}:{}),
            ...(Object.keys(object(metadata.discovery_readiness)).length?{discoveryReadiness:object(metadata.discovery_readiness)}:{}),
            ...(Object.keys(object(metadata.error)).length?{readError:object(metadata.error)}:{}),
            ...(Object.keys(object(metadata.scope_observation)).length?{scopeObservation:object(metadata.scope_observation)}:{}),
            ...(typeof metadata.requested_mode==='string'?{requestedMode:metadata.requested_mode}:{}),
            ...(metadata.publication!==undefined?{publication:metadata.publication}:{}),
            ...(Object.keys(coverage).length?{coverage}:{}),...(typeof metadata.count_unit==='string'?{countUnit:metadata.count_unit}:{}),
            files:files.slice(0,100).flatMap(value=>{const file=object(value);return typeof file.path==='string'&&Number.isSafeInteger(file.count)&&Number(file.count)>=0?[{path:file.path,count:Number(file.count)}]:[];}),
            filesTruncated:metadata.files_truncated===true,more:metadata.more===true||continuationCursor(report.result)!==undefined,...(typeof metadata.incomplete_reason==='string'?{incompleteReason:metadata.incomplete_reason}:{}),
            suggestions:suggestions.slice(0,100).flatMap(value=>{const target=object(value),citation=object(target.citation);return typeof target.target==='string'?[{target:target.target,...(typeof target.symbol_id==='string'?{symbolId:target.symbol_id}:{}),...(typeof citation.path==='string'?{path:citation.path}:{}),...(typeof target.coverage==='string'?{coverage:target.coverage}:{})}]:[];}),
            definitions:definitions.slice(0,100).map(object),omittedFiles:Math.max(0,files.length-100),omittedSuggestions:Math.max(0,suggestions.length-100),omittedDefinitions:Math.max(0,definitions.length-100),
            ...(continuationRequiredRecordBytes(report.result)!==undefined?{requiredRecordBytes:continuationRequiredRecordBytes(report.result)}:{})}];
    });
}
/** User-selected follow-up, validated against retained metadata and the exact original publication. */
export function discoveryFollowupPlan(envelope:Value,step:string,target:string,path?:string):Value {
    const prior=object(envelope.plan),details=discoveryDetails(envelope).find(value=>value.id===step);
    const report=(Array.isArray(envelope.steps)?envelope.steps:[]).map(object).find((value,i)=>String(value.id??`step-${i}`)===step);
    if(!report||!details||!details.canFollow)throw new Error('Only a completed evidence step from the same publication can be followed. Re-run this result to collect current evidence.');
    const planPrefix=typeof prior.path_prefix==='string'?prior.path_prefix:'',sourceArgs=object(report.arguments);
    const stepPrefix=typeof sourceArgs.path_prefix==='string'?sourceArgs.path_prefix:planPrefix;
    const prefix=stepPrefix.startsWith(planPrefix)?stepPrefix:planPrefix.startsWith(stepPrefix)?planPrefix:undefined;
    if(prefix===undefined)throw new Error('The retained step and plan scopes do not agree. Re-run a scoped investigation.');
    let tool:string,args:Value;
    if(path!==undefined){
        if(details.tool!=='search')throw new Error('Choose a definition pointer from a completed search.');
        const hint=details.definitions.find(value=>value.name===target&&Array.isArray(value.matches)&&value.matches.some(raw=>object(object(raw).citation).path===path));
        const match=(Array.isArray(hint?.matches)?hint.matches:[]).map(object).find(value=>object(value.citation).path===path);
        if(!match||typeof match.symbol_id!=='string'||!match.symbol_id)throw new Error('Choose an exact definition pointer with a symbol identity from the retained result.');
        tool='symbols';args={name:match.symbol_id,path};
    }else{
        if(details.tool!=='relationships')throw new Error('Choose a written target from a completed relationship result.');
        if(!details.suggestions.some(value=>value.target===target))throw new Error('Choose a written target from the retained result.');
        tool='relationships';args={symbol:target,direction:'incoming',kind:'call',path_prefix:prefix};
    }
    return {schema:'airk.parse.plan/1',publication:envelope.publication,collection:prior.collection,path_prefix:prefix,steps:[{id:'followup',tool,args}]};
}

/** Rich native plan records retain their complete citation and original location. */
export function evidenceCards(envelope: Value): EvidenceCard[] {
    const rows = Array.isArray(envelope.evidence) ? envelope.evidence : [];
    return rows.flatMap((raw, i) => {
        const row = object(raw), citation = object(row.citation), path = citation.path, hash = citation.content_hash;
        if (typeof path !== 'string' || typeof hash !== 'string' || !/^[0-9a-f]{64}$/.test(hash) || !Number.isSafeInteger(citation.start_line) || !Number.isSafeInteger(citation.end_line)) { return []; }
        return [{ id: `record-${i}`, path, hash, start: Number(citation.start_line), end: Number(citation.end_line), text: typeof row.text === 'string' ? row.text : '', location: citation.location,
            step: String(row.step ?? ''), publication: typeof row.publication === 'string' ? row.publication : undefined }];
    });
}
/** A display-only cache owned by one view, bounded to its current transcript.
 * Completed native envelopes are replaced, never mutated by WorkbenchView.
 * No export, continuation admission, citation validation or history read uses it. */
export const DISPLAY_PROJECTION_TURNS=200;
export class CurrentTurnProjections {
    private readonly retained=new Map<WorkbenchTurn,{envelope:Value|undefined;cards:EvidenceCard[];continuations:Continuation[]}>();
    project(turns:WorkbenchTurn[]):{cards:Record<string,EvidenceCard[]>;continuations:Record<string,Continuation[]>} {
        const current=new Set(turns.slice(-DISPLAY_PROJECTION_TURNS));
        for(const turn of this.retained.keys())if(!current.has(turn))this.retained.delete(turn);
        const cards:Array<[string,EvidenceCard[]]>=[],pages:Array<[string,Continuation[]]>=[];
        for(const turn of turns){
            let value=this.retained.get(turn);
            if(!value||value.envelope!==turn.envelope){
                value={envelope:turn.envelope,cards:evidenceCards(turn.envelope??{}),continuations:continuations(turn.envelope??{})};
                if(current.has(turn))this.retained.set(turn,value);
            }
            cards.push([turn.id,value.cards]);pages.push([turn.id,value.continuations]);
        }
        return {cards:Object.fromEntries(cards),continuations:Object.fromEntries(pages)};
    }
    clear():void{this.retained.clear();}
}
export function canonical(value: unknown): string {
    const sorted = (v: unknown): unknown => Array.isArray(v) ? v.map(sorted) : v && typeof v === 'object'
        ? Object.fromEntries(Object.entries(v).sort(([a],[b]) => a<b?-1:a>b?1:0).map(([k,item]) => [k,sorted(item)])) : v;
    return JSON.stringify(sorted(value),null,2);
}
/** Selection exports retain uncertainty for their own source steps, without other result payloads. */
function selectedSourceCoverage(turn:WorkbenchTurn,selected:Set<string>):Value[]{
    const steps=new Set(evidenceCards(turn.envelope??{}).filter(card=>selected.has(card.id)).map(card=>card.step));
    return (Array.isArray(turn.envelope?.steps)?turn.envelope.steps:[]).flatMap(raw=>{
        const report=object(raw);if(typeof report.id!=='string'||!steps.has(report.id))return [];
        const metadata=resultMetadata(report.result),qualification:Value={};
        if(typeof metadata.coverage==='string')qualification.coverage=metadata.coverage;
        else {
            const coverage=object(metadata.coverage),safe:Value={};
            for(const key of ['basis','line_completeness'])if(typeof coverage[key]==='string')safe[key]=coverage[key];
            for(const key of ['candidate_scan_complete','query_complete','token_boundary_complete'])if(typeof coverage[key]==='boolean')safe[key]=coverage[key];
            if(coverage.query_total_matches===null)safe.query_total_matches=null;
            if(Object.keys(safe).length)qualification.coverage=safe;
        }
        if(typeof metadata.truncated==='boolean')qualification.truncated=metadata.truncated;
        if(typeof metadata.count_unit==='string')qualification.count_unit=metadata.count_unit;
        if(typeof metadata.resolution==='string')qualification.resolution=metadata.resolution;
        if(metadata.reply_detail==='lean'||metadata.reply_detail==='rich')qualification.reply_detail=metadata.reply_detail;
        if(metadata.guidance)qualification.guidance=metadata.guidance;
        // These observations qualify the selected evidence's original request,
        // not merely its saved bytes. Retain complete native objects, including
        // unknown future fields, without deriving a fresh availability claim.
        for(const key of ['execution','semantic_readiness','discovery_readiness','error','requested_mode','publication','read_guidance','scope_observation']){
            if(metadata[key]!==undefined)qualification[key]=metadata[key];
        }
        return Object.keys(qualification).length?[{step:report.id,tool:typeof report.tool==='string'?report.tool:'',scope:'original_source_request',...qualification}]:[];
    });
}
/** One selected response contract shared by JSON and Markdown copies/exports. */
function selectedEnvelope(turn:WorkbenchTurn,selected:Set<string>):Value {
    const sourceCoverage=selectedSourceCoverage(turn,selected);
    const selectedCards=evidenceCards(turn.envelope!).filter(card=>selected.has(card.id));
    const checks=selectedVerification(turn.envelope!,selectedCards);
    const {source_verification:_originalReceipt,...original}=turn.envelope!;
    return {...original,steps:[],evidence:selectedCards.map(card=>(turn.envelope!.evidence as unknown[])[Number(card.id.slice('record-'.length))]),selection:true,
        ...(checks.requested?{source_verification_observations:checks.observations,selected_verification_accounting:checks.accounting}:{}),...(sourceCoverage.length?{source_coverage:sourceCoverage}:{})};
}
export function markdown(turn: WorkbenchTurn, selected?: Set<string>): string {
    const escape=(text:string)=>text.replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('>','&gt;').replace(/[\\`*_[\]]/g,'\\$&');
    const fenced=(text:string)=>{const fence='`'.repeat(Math.max(3,...[...text.matchAll(/`+/g)].map(m=>m[0].length+1)));return `${fence}\n${text}\n${fence}`;};
    const e = turn.envelope ?? {}, cards = evidenceCards(e).filter(card => !selected || selected.has(card.id));
    const content = cards.map(card => {
        return `### ${escape(card.path)}:${card.start}-${card.end}\n\nLocation: ${escape(JSON.stringify(card.location??null))}\n\nSource hash: ${card.hash}\n\n${fenced(card.text)}`;
    });
    const sourceCoverage=selected?selectedSourceCoverage(turn,selected):[];
    const selectedRows=selected?cards.map(card=>(e.evidence as unknown[])[Number(card.id.slice('record-'.length))]):[];
    const selectedChecks=selected?selectedVerification(e,cards):undefined;
    const metadata=selected?(sourceCoverage.length?[...sourceCoverage.flatMap(value=>{const label=searchExecutionLabel(value.execution);return label?[escape(label)]:[];}),`### Original source request coverage for selected evidence\n\n${fenced(canonical(sourceCoverage))}`]:[]):(Array.isArray(e.steps)?e.steps:[]).flatMap(raw=>{
        const report=object(raw),details=resultMetadata(report.result);
        const label=searchExecutionLabel(details.execution);
        return Object.keys(details).length?[...(label?[escape(label)]:[]),`### ${escape(String(report.tool??report.id??'Step'))} discovery details\n\n${fenced(canonical(details))}`]:[];
    });
    return [`## ${escape(turn.input.replaceAll('\n',' '))}`, `Status: ${escape(String(e.status ?? turn.error ?? 'trimmed'))}`, `Publication: ${escape(String(e.publication ?? 'not available'))}`,
        `Scope: ${escape(String(object(e.plan).path_prefix ?? ''))}`, ...(turn.trimmed ? ['Saved result was trimmed; re-run to collect current evidence.'] : []), ...content,
        ...(e.scope_observation!==undefined?[`### Recorded scope observation\n\n${fenced(canonical(e.scope_observation))}`]:[]),
        ...(selected?[`### ${selectedChecks?.requested?'Selected evidence and dated source observations':'Selected evidence'}\n\n${fenced(canonical({evidence:selectedRows,...(selectedChecks?.requested?{source_verification_observations:selectedChecks.observations,selected_verification_accounting:selectedChecks.accounting}:{})}))}`]:[]),
        ...metadata,...(turn.envelope ? [`### ${selected?'Complete selected response':'Complete response'}\n\n${fenced(canonical(selected?selectedEnvelope(turn,selected):e))}`] : []),
        ...(turn.nativeFailure?[`### Complete original native refusal\n\n${fenced(canonical(turn.nativeFailure))}`]:[]),
        `Warnings: ${escape(JSON.stringify(e.warnings ?? []))}`, `Completeness: ${escape(JSON.stringify(e.completeness ?? []))}`,
        ...(e.source_verification?[verificationLabel(e.source_verification),'Saved-byte observations retain their original dates. Extraction, coverage, relevance and unsaved buffers remain unverified; recheck before later edits.']:['Untrusted indexed evidence. Verify relevant live files before edits.'])].join('\n\n');
}
export function renderExport(session: WorkbenchSession, format: 'json'|'markdown', turnId?: string, selection?: string[]): string {
    const turns = session.turns.filter(turn => !turnId || turn.id === turnId);
    if (format === 'markdown') { return turns.map(turn => markdown(turn, selection ? new Set(selection) : undefined)).join('\n\n---\n\n'); }
    return canonical({ schema:'airk.workbench.export/1', project:session.project, omitted_turns:session.omitted, turns:turns.map(turn => {
        if (!selection || !turn.envelope) { return turn; }
        return {...turn,envelope:selectedEnvelope(turn,new Set(selection))};
    }) });
}
