// SPDX-License-Identifier: Apache-2.0
import {nativeVerification,readResultBody,selectedVerification,verificationLabel} from './sourceVerification';
import { CapabilityFieldDisplay, Continuation, EvidenceCard, object, Value, WorkbenchSession, WorkbenchTurn, WORKBENCH_MAX_STEP_REPORTS } from './workbenchContracts';

export function workbenchStepReportOmissions(envelope:Value):number {
    return Array.isArray(envelope.steps)?Math.max(0,envelope.steps.length-WORKBENCH_MAX_STEP_REPORTS):0;
}
const stepReports=(envelope:Value):unknown[]=>Array.isArray(envelope.steps)?envelope.steps.slice(0,WORKBENCH_MAX_STEP_REPORTS):[];

export const SHARED_CAPABILITY_REFERENCE='$airk_shared_capability_fields';
const INLINE_CAPABILITY_FIELDS=new Set(['request_admission','runtime','work','scope_observation','publication','freshness','collection','generation','observed_unix_ms','cache_age_ms','cached','status_observation','request_id','query_concurrency','source_verification','error','requested_scope']);
export function shareableCapabilityField(key:string):boolean {
    return key.length>0&&key.length<=128&&/^[\x00-\x7f]+$/.test(key)&&!['__proto__','constructor','prototype'].includes(key)
        &&!INLINE_CAPABILITY_FIELDS.has(key)&&!key.startsWith('runtime')&&!key.endsWith('_observation')&&key!==SHARED_CAPABILITY_REFERENCE;
}
/** This merge is display-only and accepts only a separately host-verified table.
 * No reference grants read authority or replaces the complete original result. */
export function materializeCapabilityMember(raw:unknown,display?:CapabilityFieldDisplay):Value {
    const body=readResultBody(raw);if(!Object.hasOwn(body,SHARED_CAPABILITY_REFERENCE))return body;
    if(display?.state!=='verified')throw new Error(display?.state==='unavailable'?display.error:'Shared capability data has not been validated.');
    const reference=body[SHARED_CAPABILITY_REFERENCE];
    if(typeof reference!=='string'||!/^[0-9a-f]{64}$/.test(reference)||!Object.hasOwn(display.values,reference))throw new Error('Shared capability reference is unavailable.');
    const fields=display.values[reference],inline={...body};delete inline[SHARED_CAPABILITY_REFERENCE];
    if(Object.keys(fields).some(key=>!shareableCapabilityField(key)||Object.hasOwn(inline,key)))throw new Error('Shared capability fields conflict with native member fields.');
    return {...fields,...inline};
}

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
    indexId?:string;projectIds?:string[];
    replyDetail?:'lean'|'rich';
    guidance?:Value;
    readGuidance?:Value;
    profileFamilies?:Value;
    recommendations?:Value;
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
    if(report.combined_origin!==undefined){
        const origin=object(report.combined_origin);
        return envelope.executed===true&&['complete','partial'].includes(String(envelope.status))&&report.status==='complete'
            &&typeof token==='string'&&token===object(origin.publication).token
            &&typeof origin.index_id==='string'&&/^idx_[0-9a-f]{32}$/.test(origin.index_id)&&Array.isArray(origin.project_ids)&&origin.project_ids.length>0&&origin.project_ids.length<=32
            &&origin.project_ids.every(id=>typeof id==='string'&&/^prj_[0-9a-f]{32}$/.test(id));
    }
    return envelope.executed===true&&['complete','partial'].includes(String(envelope.status))&&report.status==='complete'&&typeof envelope.publication==='string'&&envelope.publication.length>0&&token===envelope.publication;
}
function discoveryReports(envelope:Value):Value[] {
    return stepReports(envelope).flatMap((raw,i)=>{
        const report=object(raw),combined=readResultBody(report.result);
        if(combined.schema!=='airk.combined-read/1')return [{...report,id:String(report.id??`step-${i}`)}];
        const selected=object(combined.selection),selections=Array.isArray(selected.members)?selected.members.slice(0,32).map(object):[];
        const publications=Array.isArray(object(combined.publication_vector).members)?(object(combined.publication_vector).members as unknown[]).map(object):[];
        return (Array.isArray(combined.members)?combined.members.slice(0,32):[]).map((raw,j)=>{
            const member=object(raw),body=readResultBody(member.result),observation=object(body.scope_observation??object(member.result).scope_observation),grant=object(observation.grant);
            let projects:string[]=[];
            if(Array.isArray(selected.index_ids)){
                const matching=Array.isArray(observation.members)?observation.members.slice(0,32).map(object).filter(row=>row.index_id===member.index_id):[];
                if(selected.index_ids.includes(member.index_id)&&observation.schema===1&&observation.qualifier==='selected_subset'
                    &&grant.id===selected.grant_id&&grant.revision===selected.grant_revision&&matching.length===1&&Array.isArray(matching[0].memberships)&&matching[0].memberships.length<=32){
                    const ids=matching[0].memberships.map(raw=>object(raw).project_id);
                    if(ids.length>0&&new Set(ids).size===ids.length&&ids.every(id=>typeof id==='string'&&/^prj_[0-9a-f]{32}$/.test(id)&&Array.isArray(selected.project_ids)&&selected.project_ids.includes(id)))projects=(ids as string[]).sort();
                }
            }else{
                // Historical saved results used the full resolved selection.
                // Missing current member observations never imply all Projects.
                const selection=selections.find(row=>row.index_id===member.index_id),frozen=Array.isArray(selection?.memberships)?selection.memberships.slice(0,32):[];
                projects=frozen.map(raw=>object(object(raw).membership).project_id).filter((value):value is string=>typeof value==='string');
            }
            return {...report,id:`member-${i}-${j}`,result:body,status:report.status==='complete'&&member.outcome==='complete'?'complete':'failed',
                combined_origin:{index_id:member.index_id,project_ids:projects,publication:publications.find(row=>row.index_id===member.index_id)?.publication}};
        });
    });
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
        profile_families:Object.keys(object(source.profile_families)).length?source.profile_families:undefined,
        recommendations:source.schema==='airk.recommendations/1'?source:undefined,
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
    return discoveryReports(envelope).flatMap((raw,i)=>{
        const report=object(raw),metadata=resultMetadata(report.result);
        if(!Object.keys(metadata).length)return [];
        const files=Array.isArray(metadata.files)?metadata.files:[],suggestions=Array.isArray(metadata.suggested_targets)?metadata.suggested_targets:[],definitions=Array.isArray(metadata.definitions)?metadata.definitions:[];
        const count=object(metadata.match_count),coverage=object(metadata.coverage);
        const origin=object(report.combined_origin);
        return [{id:String(report.id??`step-${i}`),tool:String(report.tool??''),canFollow:canFollowResult(envelope,report),...(Object.keys(count).length?{count}:{}),
            ...(typeof origin.index_id==='string'?{indexId:origin.index_id}:{}),...(Array.isArray(origin.project_ids)&&origin.project_ids.length?{projectIds:origin.project_ids.map(String)}:{}),
            ...(metadata.reply_detail==='lean'||metadata.reply_detail==='rich'?{replyDetail:metadata.reply_detail}:{}),
            ...(metadata.guidance?{guidance:object(metadata.guidance)}:{}),
            ...(metadata.read_guidance?{readGuidance:object(metadata.read_guidance)}:{}),
            ...(metadata.profile_families?{profileFamilies:object(metadata.profile_families)}:{}),
            ...(metadata.recommendations?{recommendations:object(metadata.recommendations)}:{}),
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
    const report=discoveryReports(envelope).find(value=>value.id===step);
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
    const origin=object(report.combined_origin);
    if(report.combined_origin!==undefined){
        if(typeof origin.index_id!=='string'||!/^idx_[0-9a-f]{32}$/.test(origin.index_id)||!Array.isArray(origin.project_ids)||!origin.project_ids.length
            ||origin.project_ids.some(id=>typeof id!=='string'||!/^prj_[0-9a-f]{32}$/.test(id)))throw new Error('The originating index is ambiguous. Choose qualified evidence and prepare a new request.');
        return {schema:'airk.parse.plan/1',query_scope:{project_ids:origin.project_ids,index_ids:[origin.index_id]},path_prefix:prefix,
            steps:[{id:'followup',tool,args:{...args,origin_publication:object(origin.publication).token}}]};
    }
    return {schema:'airk.parse.plan/1',publication:envelope.publication,collection:prior.collection,path_prefix:prefix,steps:[{id:'followup',tool,args}]};
}

/** Rich native plan records retain their complete citation and original location. */
export function evidenceRows(envelope:Value):Value[] {
    if(Array.isArray(envelope.evidence))return envelope.evidence.map(object);
    const pointer=(root:Value,path:unknown):unknown=>{
        if(typeof path!=='string'||path.length>4096||!path.startsWith('/'))return;
        let value:unknown=root;
        for(const escaped of path.slice(1).split('/')){
            if(/~[^01]|~$/.test(escaped))return;
            const key=escaped.replaceAll('~1','/').replaceAll('~0','~'),row=object(value);
            if(Array.isArray(value)){if(!/^(0|[1-9][0-9]*)$/.test(key)||Number(key)>=value.length)return;value=value[Number(key)];}
            else {if(!Object.hasOwn(row,key))return;value=row[key];}
        }
        return value;
    };
    const combined=(value:Value,step:string):Value[]=>{
        if(value.schema!=='airk.combined-read/1'||!Array.isArray(value.ranking)||value.ranking.length>100)return [];
        return value.ranking.flatMap(raw=>{
            const rank=object(raw),selected=pointer(value,rank.result_pointer),row=typeof selected==='string'?{path:selected}:object(selected),origin=object(rank.origin);
            const memberPath=typeof rank.result_pointer==='string'?/^\/members\/(0|[1-9][0-9]*)\/result(?:\/structuredContent)?(?:\/|$)/.exec(rank.result_pointer):undefined;
            const member=memberPath&&Array.isArray(value.members)?object(value.members[Number(memberPath[1])]):undefined;
            if(!member||member.index_id!==rank.index_id||member.outcome!=='complete')return [];
            if(typeof origin.index_id!=='string'||!/^idx_[0-9a-f]{32}$/.test(origin.index_id)||origin.index_id!==rank.index_id
                ||!Array.isArray(origin.project_ids)||!origin.project_ids.length||origin.project_ids.length>32
                ||origin.project_ids.some(id=>typeof id!=='string'||!/^prj_[0-9a-f]{32}$/.test(id)))return [];
            return [{...row,step,origin,publication:object(origin.publication).token,result_pointer:rank.result_pointer}];
        });
    };
    if(envelope.schema==='airk.combined-read/1')return combined(envelope,'');
    return stepReports(envelope).flatMap(raw=>{const step=object(raw);return combined(object(step.result),String(step.id??''));});
}
/** Complete member responses stay native and immutable through details/export. */
export function combinedReadDetails(envelope:Value):Array<{step:string;result:Value}> {
    const verification=readResultBody(envelope.member_verification),checks=['airk.combined-verification/1','airk.combined-read/1'].includes(String(verification.schema))?[{step:'Returned-source verification',result:verification}]:[];
    if(envelope.schema==='airk.combined-read/1')return [{step:'',result:envelope},...checks];
    return [...stepReports(envelope).flatMap(raw=>{const step=object(raw),result=readResultBody(step.result);return result.schema==='airk.combined-read/1'?[{step:String(step.id??''),result}]:[];}),...checks];
}
export function evidenceCards(envelope: Value): EvidenceCard[] {
    const rows = evidenceRows(envelope);
    return rows.flatMap((raw, i) => {
        const row = object(raw), citation = object(row.citation), path = citation.path, hash = citation.content_hash;
        if (typeof path !== 'string' || typeof hash !== 'string' || !/^[0-9a-f]{64}$/.test(hash) || !Number.isSafeInteger(citation.start_line) || !Number.isSafeInteger(citation.end_line)
            ||Number(citation.start_line)<1||Number(citation.end_line)<Number(citation.start_line)) { return []; }
        const origin=object(row.origin);
        return [{ id: `record-${i}`, path, hash, start: Number(citation.start_line), end: Number(citation.end_line), text: typeof row.text === 'string' ? row.text : '', location: citation.location,
            step: String(row.step ?? ''), publication: typeof row.publication === 'string' ? row.publication : undefined,
            ...(typeof origin.index_id==='string'?{indexId:origin.index_id}:{}),...(Array.isArray(origin.project_ids)?{projectIds:origin.project_ids.map(String)}:{}),
            ...(typeof origin.source_root_id==='string'?{sourceRootId:origin.source_root_id}:{}),...(typeof origin.source_relative_path==='string'?{sourceRelativePath:origin.source_relative_path}:{}),
            ...(typeof row.result_pointer==='string'?{resultPointer:row.result_pointer}:{}) }];
    });
}
/** A display-only cache owned by one view, bounded to its current transcript.
 * Completed native envelopes are replaced, never mutated by WorkbenchView.
 * No export, continuation admission, citation validation or history read uses it. */
export const DISPLAY_PROJECTION_TURNS=200;
export class CurrentTurnProjections {
    private readonly retained=new Map<WorkbenchTurn,{envelope:Value|undefined;cards:EvidenceCard[];continuations:Continuation[]}>();
    project(turns:WorkbenchTurn[],resolve:(turn:WorkbenchTurn)=>Value|undefined=turn=>turn.envelope):{cards:Record<string,EvidenceCard[]>;continuations:Record<string,Continuation[]>} {
        const current=new Set(turns.slice(-DISPLAY_PROJECTION_TURNS));
        for(const turn of this.retained.keys())if(!current.has(turn))this.retained.delete(turn);
        const cards:Array<[string,EvidenceCard[]]>=[],pages:Array<[string,Continuation[]]>=[];
        for(const turn of turns){
            const envelope=resolve(turn);
            let value=this.retained.get(turn);
            if(!value||value.envelope!==envelope){
                value={envelope,cards:evidenceCards(envelope??{}),continuations:continuations(envelope??{})};
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
    const combined=combinedReadDetails(turn.envelope??{}).filter(detail=>steps.has(detail.step)).map(({step,result})=>({step,scope:'original_combined_request',
        ...(result.shared_values!==undefined?{shared_values:result.shared_values}:{}),selection:result.selection,publication_vector:result.publication_vector,rank_policy:result.rank_policy,partial:result.partial,truncated:result.truncated,
        ...(result.error!==undefined?{error:result.error}:{}),members:(Array.isArray(result.members)?result.members:[]).map(raw=>{
            const member=object(raw);return {index_id:member.index_id,allocation:member.allocation,outcome:member.outcome,...(member.cutoff!==undefined?{cutoff:member.cutoff}:{}),
                result_metadata:resultMetadata(member.result),result_payload_omitted:true};})}));
    return [...combined,...(Array.isArray(turn.envelope?.steps)?turn.envelope.steps:[]).flatMap(raw=>{
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
        for(const key of ['execution','semantic_readiness','discovery_readiness','error','requested_mode','publication','read_guidance','profile_families','scope_observation']){
            if(metadata[key]!==undefined)qualification[key]=metadata[key];
        }
        return Object.keys(qualification).length?[{step:report.id,tool:typeof report.tool==='string'?report.tool:'',scope:'original_source_request',...qualification}]:[];
    })];
}
/** One selected response contract shared by JSON and Markdown copies/exports. */
function selectedEnvelope(turn:WorkbenchTurn,selected:Set<string>,projection=turn.envelope!):Value {
    const raw=turn.envelope!;turn={...turn,envelope:projection};
    const sourceCoverage=selectedSourceCoverage(turn,selected);
    const originalEncoding=combinedReadDetails(raw).filter(({result})=>result.shared_values!==undefined).map(({step,result})=>({step,shared_values:result.shared_values,publication_vector:result.publication_vector,members:(Array.isArray(result.members)?result.members:[]).map(item=>{const member=object(item),body=readResultBody(member.result);return {index_id:member.index_id,...(body.$airk_shared_native_header_fields!==undefined?{$airk_shared_native_header_fields:body.$airk_shared_native_header_fields}:{}),...(body.$airk_shared_capability_fields!==undefined?{$airk_shared_capability_fields:body.$airk_shared_capability_fields}:{})};})}));
    const selectedCards=evidenceCards(turn.envelope!).filter(card=>selected.has(card.id));
    const checks=selectedVerification(turn.envelope!,selectedCards);
    const {source_verification:_originalReceipt,member_verification:_memberReceipt,delivery_observation:_delivery,...original}=raw;
    const members=Array.isArray(original.members)?original.members.map(raw=>{const {result:_result,...member}=object(raw);return {...member,result_payload_omitted:true};}):undefined;
    return {...original,...(originalEncoding.length?{original_native_metadata:originalEncoding}:{}),steps:[],...(members?{members,ranking:[],original_member_payloads_omitted:true}:{}),evidence:selectedCards.map(card=>evidenceRows(turn.envelope!)[Number(card.id.slice('record-'.length))]),
        ...(original.selection!==null&&typeof original.selection==='object'&&!Array.isArray(original.selection)?{selected_subset:true}:{selection:true}),
        ...(checks.requested?{source_verification_observations:checks.observations,selected_verification_accounting:checks.accounting}:{}),...(sourceCoverage.length?{source_coverage:sourceCoverage}:{})};
}
export function markdown(turn: WorkbenchTurn, selected?: Set<string>,projection=turn.envelope): string {
    const escape=(text:string)=>text.replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('>','&gt;').replace(/[\\`*_[\]]/g,'\\$&');
    const fenced=(text:string)=>{const fence='`'.repeat(Math.max(3,...[...text.matchAll(/`+/g)].map(m=>m[0].length+1)));return `${fence}\n${text}\n${fence}`;};
    const e = projection ?? {}, cards = evidenceCards(e).filter(card => !selected || selected.has(card.id));
    const content = cards.map(card => {
        return `### ${escape(card.path)}:${card.start}-${card.end}\n\nLocation: ${escape(JSON.stringify(card.location??null))}\n\nSource hash: ${card.hash}\n\n${fenced(card.text)}`;
    });
    const sourceCoverage=selected?selectedSourceCoverage({...turn,envelope:e},selected):[];
    const selectedRows=selected?cards.map(card=>evidenceRows(e)[Number(card.id.slice('record-'.length))]):[];
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
        ...metadata,...(turn.envelope ? [`### ${selected?'Complete selected response':'Complete response'}\n\n${fenced(canonical(selected?selectedEnvelope(turn,selected,e):turn.envelope))}`] : []),
        ...(turn.nativeFailure?[`### Complete original native refusal\n\n${fenced(canonical(turn.nativeFailure))}`]:[]),
        `Warnings: ${escape(JSON.stringify(e.warnings ?? []))}`, `Completeness: ${escape(JSON.stringify(e.completeness ?? []))}`,
        ...(e.source_verification?[verificationLabel(e.source_verification),'Saved-byte observations retain their original dates. Extraction, coverage, relevance and unsaved buffers remain unverified; recheck before later edits.']:['Untrusted indexed evidence. Verify relevant live files before edits.'])].join('\n\n');
}
export function renderExport(session: WorkbenchSession, format: 'json'|'markdown', turnId?: string, selection?: string[],resolve:(turn:WorkbenchTurn)=>Value|undefined=turn=>turn.envelope): string {
    const turns = session.turns.filter(turn => !turnId || turn.id === turnId);
    if (format === 'markdown') { return turns.map(turn => markdown(turn, selection ? new Set(selection) : undefined,resolve(turn))).join('\n\n---\n\n'); }
    return canonical({ schema:'airk.workbench.export/1', project:session.project, omitted_turns:session.omitted, turns:turns.map(turn => {
        if (!selection || !turn.envelope) { return turn; }
        return {...turn,envelope:selectedEnvelope(turn,new Set(selection),resolve(turn)??{})};
    }) });
}
