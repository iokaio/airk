// SPDX-License-Identifier: Apache-2.0
/** Dated native observations remain inert evidence; viewing/exporting never checks a file. */
import {EvidenceCard,object,Value,WORKBENCH_MAX_STEP_REPORTS} from './workbenchContracts';

export function nativeVerification(raw:unknown):Value|undefined {
    const value=object(raw);
    return value.schema==='airk.saved-source-verification/1'&&value.method==='sha256-full-saved-source/1'
        &&Array.isArray(value.sources)&&value.sources.length<=256&&Array.isArray(value.references)&&value.references.length<=4096
        &&Array.isArray(value.origins)&&value.origins.length<=32?value:undefined;
}
export function verificationLabel(raw:unknown):string {
    const value=nativeVerification(raw);
    if(!value)return 'Saved-source verification unavailable.';
    const counts=object(value.counts),sources=Array.isArray(value.sources)?value.sources.map(object):[];
    const at=value.observed_to_unix_ms;
    const dated=Number.isSafeInteger(at)&&Number(at)>=0&&Number(at)<=8640000000000000&&Number.isSafeInteger(value.observed_from_unix_ms)&&Number(value.observed_from_unix_ms)>=0&&Number(value.observed_from_unix_ms)<=Number(at);
    const date=dated?new Date(Number(at)).toISOString():'unknown time';
    const references=Array.isArray(value.references)?value.references.map(object):[];
    const origins=Array.isArray(value.origins)?value.origins.map(object):[];
    const referenceIds=new Set(references.map(reference=>reference.id));
    const matched=Number.isSafeInteger(counts.matched_sources)&&Number(counts.matched_sources)>=0?Number(counts.matched_sources):0;
    const total=Number.isSafeInteger(counts.requested_sources)&&Number(counts.requested_sources)>=0?String(counts.requested_sources):'unknown';
    const validMatch=dated&&value.status==='verified'&&value.requirements_satisfied===true&&value.inventory_complete===true
        &&value.all_references_accounted===true&&value.source_rows_omitted===0&&value.publication_fence==='current'
        &&matched>0&&matched===sources.length&&matched===counts.requested_sources&&counts.unchecked_sources===0&&counts.checked_sources===matched&&counts.mismatched_sources===0
        &&counts.references===references.length&&referenceIds.size===references.length&&new Set(sources.map(source=>source.id)).size===sources.length
        &&references.every(reference=>sources.some(source=>source.id===reference.source_id&&Array.isArray(source.reference_ids)&&source.reference_ids.includes(reference.id)))
        &&new Set(origins.map(origin=>origin.id)).size===origins.length
        &&sources.every(source=>source.state==='matched'&&origins.some(origin=>origin.id===source.origin_id&&typeof origin.publication==='string'&&origin.publication.startsWith('airk-r1:'))
            &&Number.isSafeInteger(source.observed_from_unix_ms)&&Number.isSafeInteger(source.observed_to_unix_ms)
            &&Number(source.observed_from_unix_ms)>=Number(value.observed_from_unix_ms)&&Number(source.observed_to_unix_ms)>=Number(source.observed_from_unix_ms)&&Number(source.observed_to_unix_ms)<=Number(at)
            &&typeof source.original_root==='string'&&/^(?:\/|[A-Za-z]:[\\/]|\\\\)/.test(source.original_root)
            &&typeof source.source_relative_path==='string'&&source.source_relative_path.length>0&&!/[\\:\u0000]/.test(source.source_relative_path)&&source.source_relative_path.split('/').every(part=>!['','.','..'].includes(part))
            &&typeof source.expected_hash==='string'&&/^[0-9a-f]{64}$/.test(source.expected_hash)&&source.expected_hash===source.observed_hash
            &&Array.isArray(source.reference_ids)&&source.reference_ids.length>0&&source.reference_ids.every(id=>referenceIds.has(id)));
    if(validMatch)return `Saved sources verified at ${date}: ${matched} referenced source${matched===1?'':'s'} matched the indexed bytes.`;
    if(value.status==='not_applicable'&&counts.references===0&&counts.requested_sources===0&&references.length===0&&sources.length===0&&value.inventory_complete===true)return `No returned saved-source references to verify (${date}).`;
    return `Saved-source check at ${date}: ${matched} of ${total} referenced sources matched; verification incomplete.`;
}

/** Classify only a source bound to this exact native file-info result. No host bytes are read. */
export function nativeSavedSourceState(raw:unknown,expectedHash:string,publication:string|undefined):'matches_index'|'changed_since_index'|'missing'|'unchecked'{
    const info=object(raw),location=object(info.source),receipt=nativeVerification(info.source_verification);
    if(!receipt||!publication||info.content_hash!==expectedHash||!/^[0-9a-f]{64}$/.test(expectedHash)||typeof location.root!=='string'||typeof location.relative_path!=='string')return 'unchecked';
    const origins=(receipt.origins as unknown[]).map(object),sources=(receipt.sources as unknown[]).map(object);
    const targets=sources.filter(source=>source.expected_hash===expectedHash&&source.original_root===location.root&&source.source_relative_path===location.relative_path
        &&origins.some(origin=>origin.id===source.origin_id&&origin.publication===publication));
    if(targets.length!==1)return 'unchecked';
    const target=targets[0];
    if(target.state==='matched'&&verificationLabel(receipt).startsWith('Saved sources verified at '))return 'matches_index';
    if(target.state==='mismatch')return 'changed_since_index';
    return target.state==='missing'?'missing':'unchecked';
}

/** Unwrap actual native structured responses; parse text only in error envelopes. */
export function readResultBody(raw:unknown):Value {
    const value=object(raw);
    if(Array.isArray(value.content)&&Object.keys(object(value.structuredContent)).length)return object(value.structuredContent);
    if(value.isError===true&&Array.isArray(value.content)){
        if(Object.keys(object(value.structuredContent)).length)return object(value.structuredContent);
        const text=object(value.content[0]).text;
        if(typeof text==='string'&&text.length<=524288){try{return object(JSON.parse(text));}catch{/* Original wrapper remains in Complete response. */}}
    }
    return value;
}

export interface SelectedVerification {requested:boolean;observations:Value[];accounting:Value}

function jsonPointer(root:unknown,path:unknown):unknown {
    if(typeof path!=='string'||path.length>8192||path!==''&&!path.startsWith('/'))return;
    let value:unknown=root;
    for(const escaped of path===''?[]:path.slice(1).split('/')){
        if(/~[^01]|~$/.test(escaped))return;
        const key=escaped.replaceAll('~1','/').replaceAll('~0','~');
        if(Array.isArray(value)){if(!/^(0|[1-9][0-9]*)$/.test(key)||Number(key)>=value.length)return;value=value[Number(key)];}
        else {const row=object(value);if(!Object.hasOwn(row,key))return;value=row[key];}
    }
    return value;
}
function sameJson(left:unknown,right:unknown,depth=0):boolean {
    if(left===right)return true;
    if(depth>32)return false;
    if(Array.isArray(left)||Array.isArray(right))return Array.isArray(left)&&Array.isArray(right)&&left.length===right.length&&left.length<=4096&&left.every((item,index)=>sameJson(item,right[index],depth+1));
    if(!left||!right||typeof left!=='object'||typeof right!=='object')return false;
    const a=object(left),b=object(right),keys=Object.keys(a);
    return keys.length<=4096&&keys.length===Object.keys(b).length&&keys.every(key=>Object.hasOwn(b,key)&&sameJson(a[key],b[key],depth+1));
}
function explicitIndexAt(projection:Value,pointer:string):string|undefined {
    let current=pointer;
    for(let depth=0;depth<=32;depth++){
        const row=object(jsonPointer(projection,current)),index=object(row.origin).index_id??row.index_id;
        if(typeof index==='string')return index;
        if(!current)return;
        const slash=current.lastIndexOf('/');if(slash<0)return;current=current.slice(0,slash);
    }
    return;
}
/** Rebind full-file observations to selected citations. Do not certify a subset
 * or change the original failure/coverage/date into a new successful check. */
export function selectedVerification(envelope:Value,cards:EvidenceCard[]):SelectedVerification {
    const candidates:Array<{step?:string;index?:string;raw:unknown;body?:Value;bindings?:Value[];scope?:unknown}>=[];
    const add=(body:Value,step?:string)=>{
        candidates.push({step,raw:body.source_verification});
        if((body.schema==='airk.combined-read/1'||body.schema==='airk.combined-verification/1')&&Array.isArray(body.members)){
            const final=body.schema==='airk.combined-verification/1';
            for(const raw of body.members.slice(0,32)){
                const member=object(raw),own=readResultBody(member.result);
                candidates.push({step,index:typeof member.index_id==='string'?member.index_id:undefined,raw:own.source_verification,
                    ...(final?{body:own,bindings:Array.isArray(member.reference_bindings)&&member.reference_bindings.length<=4096?member.reference_bindings.map(object):[],scope:body.scope_identity}:{})});
            }
        }
    };
    add(envelope);
    for(const raw of Array.isArray(envelope.steps)?envelope.steps.slice(0,WORKBENCH_MAX_STEP_REPORTS):[]){const report=object(raw);add(readResultBody(report.result),typeof report.id==='string'?report.id:undefined);}
    add(readResultBody(envelope.member_verification));
    const requested=envelope.member_verification!==undefined||candidates.some(candidate=>candidate.raw!==undefined);
    const accounted=new Set<string>(),observations:Value[]=[];
    let associationWork=131072,scanComplete=true;
    for(const {step,index:memberIndex,raw,body,bindings,scope} of candidates){
        const receipt=nativeVerification(raw);
        if(!receipt)continue;
        if(bindings!==undefined&&(typeof memberIndex!=='string'||!/^idx_[0-9a-f]{32}$/.test(memberIndex)||typeof scope!=='string'||!/^[0-9a-f]{64}$/.test(scope)||scope!==envelope.scope_identity))continue;
        const originalSources=Array.isArray(receipt.sources)?receipt.sources.map(object):[],originalOrigins=Array.isArray(receipt.origins)?receipt.origins.map(object):[];
        const sources:Value[]=[],references:Value[]=[],originalAssociations:Value[]=[];
        for(let index=0;index<cards.length;index++){
            const card=cards[index];if(step!==undefined&&card.step!==step||memberIndex!==undefined&&card.indexId!==memberIndex)continue;
            const cost=originalSources.length*(originalOrigins.length+1)+(bindings===undefined?0:(receipt.references as unknown[]).length*(bindings.length+1));
            if(cost>associationWork){scanComplete=false;break;}associationWork-=cost;
            const publication=card.publication??(typeof envelope.publication==='string'?envelope.publication:undefined);
            const matches=originalSources.filter(source=>source.path===card.path&&source.expected_hash===card.hash
                &&(source.source_root_id==null||card.sourceRootId===undefined||source.source_root_id===card.sourceRootId)
                &&(bindings===undefined||card.sourceRootId!==undefined&&source.source_root_id===card.sourceRootId)
                &&publication!==undefined&&originalOrigins.some(origin=>origin.id===source.origin_id&&origin.publication===publication
                    &&(origin.index_id==null||memberIndex===undefined||origin.index_id===memberIndex)
                    &&(bindings===undefined||origin.index_id===memberIndex)));
            if(matches.length!==1)continue;
            const source=matches[0],id=`ref-${references.length}`;
            if(bindings!==undefined){
                const recorded=(receipt.references as unknown[]).map(object).find(reference=>{
                    if(reference.source_id!==source.id||!Array.isArray(source.reference_ids)||!source.reference_ids.includes(reference.id)||typeof reference.result_pointer!=='string')return false;
                    const pointer=reference.result_pointer;
                    let binding:Value|undefined;
                    for(const entry of bindings){
                        if(typeof entry.member_pointer==='string'&&typeof entry.result_pointer==='string'
                            &&entry.member_pointer.length<=4096&&entry.result_pointer.length<=4096
                            &&pointer.startsWith(entry.member_pointer)&&(pointer.length===entry.member_pointer.length||pointer[entry.member_pointer.length]==='/')
                            &&(!binding||entry.member_pointer.length>String(binding.member_pointer).length))binding=entry;
                    }
                    if(!binding)return false;
                    const outer=String(binding.result_pointer)+pointer.slice(String(binding.member_pointer).length);
                    const target=jsonPointer(envelope,outer),checked=jsonPointer(body,pointer),citation=object(target);
                    if(target==null||checked==null||!sameJson(target,checked)||explicitIndexAt(envelope,outer)!==memberIndex
                        ||citation.path!==card.path||citation.content_hash!==card.hash||citation.start_line!==card.start||citation.end_line!==card.end)return false;
                    originalAssociations.push({selected_reference_id:id,reference_id:reference.id,member_pointer:pointer,result_pointer:outer});
                    return true;
                });
                if(!recorded)continue;
            }
            let retained=sources.find(retained=>retained.id===source.id);
            if(!retained){retained={...source,reference_ids:[]};sources.push(retained);}
            (retained.reference_ids as string[]).push(id);
            references.push({id,source_id:source.id,result_pointer:`/evidence/${index}/citation`,kind:'citation'});
            accounted.add(card.id);
        }
        if(!sources.length)continue;
        const originIds=new Set(sources.map(source=>source.origin_id));
        observations.push({schema:'airk.saved-source-selected-observations/1',selection:true,verification_performed:false,
            ...(memberIndex!==undefined?{index_id:memberIndex}:{}),
            ...(bindings!==undefined?{original_member_scope_identity:scope,original_reference_associations:originalAssociations}:{}),
            original_status:receipt.status,original_counts:receipt.counts,original_requirements_satisfied:receipt.requirements_satisfied,
            original_inventory_complete:receipt.inventory_complete,original_all_references_accounted:receipt.all_references_accounted,
            original_source_rows_omitted:receipt.source_rows_omitted,original_scope_digest:receipt.scope_digest,original_selection_digest:receipt.selection_digest,
            observed_from_unix_ms:receipt.observed_from_unix_ms,observed_to_unix_ms:receipt.observed_to_unix_ms,
            method:receipt.method,limits:receipt.limits,publication_fence:receipt.publication_fence,
            origins:originalOrigins.filter(origin=>originIds.has(origin.id)),sources,references,qualifications:receipt.qualifications,
            ...(receipt.error?{original_error:receipt.error}:{})});
    }
    return {requested,observations,accounting:{scope:'selected_evidence_only',verification_performed:false,
        observation_scan_complete:scanComplete,...(!scanComplete?{limits_hit:['selected_observation_associations']}:{}),
        selected_citations:cards.length,with_retained_observations:accounted.size,
        without_retained_observations:cards.filter(card=>!accounted.has(card.id)).map(card=>card.id)}};
}
