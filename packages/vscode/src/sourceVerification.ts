// SPDX-License-Identifier: Apache-2.0
/** Dated native observations remain inert evidence; viewing/exporting never checks a file. */
import {EvidenceCard,object,Value} from './workbenchContracts';

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

/** Unwrap only an actual native error envelope, never arbitrary evidence text. */
export function readResultBody(raw:unknown):Value {
    const value=object(raw);
    if(value.isError===true&&Array.isArray(value.content)){
        if(Object.keys(object(value.structuredContent)).length)return object(value.structuredContent);
        const text=object(value.content[0]).text;
        if(typeof text==='string'&&text.length<=524288){try{return object(JSON.parse(text));}catch{/* Original wrapper remains in Complete response. */}}
    }
    return value;
}

export interface SelectedVerification {requested:boolean;observations:Value[];accounting:Value}
/** Rebind full-file observations to selected citations. Do not certify a subset
 * or change the original failure/coverage/date into a new successful check. */
export function selectedVerification(envelope:Value,cards:EvidenceCard[]):SelectedVerification {
    const requested=envelope.source_verification!==undefined||(Array.isArray(envelope.steps)?envelope.steps:[]).some(raw=>readResultBody(object(raw).result).source_verification!==undefined);
    const candidates=[{step:undefined as string|undefined,receipt:nativeVerification(envelope.source_verification)},
        ...(Array.isArray(envelope.steps)?envelope.steps:[]).map(raw=>{const report=object(raw);return {step:typeof report.id==='string'?report.id:undefined,receipt:nativeVerification(readResultBody(report.result).source_verification)};})];
    const accounted=new Set<string>(),observations:Value[]=[];
    for(const {step,receipt} of candidates){
        if(!receipt)continue;
        const originalSources=Array.isArray(receipt.sources)?receipt.sources.map(object):[],originalOrigins=Array.isArray(receipt.origins)?receipt.origins.map(object):[];
        const sources:Value[]=[],references:Value[]=[];
        for(let index=0;index<cards.length;index++){
            const card=cards[index];if(step!==undefined&&card.step!==step)continue;
            const publication=card.publication??(typeof envelope.publication==='string'?envelope.publication:undefined);
            const matches=originalSources.filter(source=>source.path===card.path&&source.expected_hash===card.hash
                &&publication!==undefined&&originalOrigins.some(origin=>origin.id===source.origin_id&&origin.publication===publication));
            if(matches.length!==1)continue;
            const source=matches[0],id=`ref-${references.length}`;
            let retained=sources.find(retained=>retained.id===source.id);
            if(!retained){retained={...source,reference_ids:[]};sources.push(retained);}
            (retained.reference_ids as string[]).push(id);
            references.push({id,source_id:source.id,result_pointer:`/evidence/${index}/citation`,kind:'citation'});
            accounted.add(card.id);
        }
        if(!sources.length)continue;
        const originIds=new Set(sources.map(source=>source.origin_id));
        observations.push({schema:'airk.saved-source-selected-observations/1',selection:true,verification_performed:false,
            original_status:receipt.status,original_counts:receipt.counts,original_requirements_satisfied:receipt.requirements_satisfied,
            original_inventory_complete:receipt.inventory_complete,original_all_references_accounted:receipt.all_references_accounted,
            original_source_rows_omitted:receipt.source_rows_omitted,original_scope_digest:receipt.scope_digest,original_selection_digest:receipt.selection_digest,
            observed_from_unix_ms:receipt.observed_from_unix_ms,observed_to_unix_ms:receipt.observed_to_unix_ms,
            method:receipt.method,limits:receipt.limits,publication_fence:receipt.publication_fence,
            origins:originalOrigins.filter(origin=>originIds.has(origin.id)),sources,references,qualifications:receipt.qualifications,
            ...(receipt.error?{original_error:receipt.error}:{})});
    }
    return {requested,observations,accounting:{scope:'selected_evidence_only',verification_performed:false,
        selected_citations:cards.length,with_retained_observations:accounted.size,
        without_retained_observations:cards.filter(card=>!accounted.has(card.id)).map(card=>card.id)}};
}
