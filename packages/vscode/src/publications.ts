// SPDX-License-Identifier: Apache-2.0
/** An observation's authority is captured by the host, never inferred from result text. */
export interface PublicationObservation { authority:string; collection:string; publication:unknown }
export type PublicationRelation='equal'|'older'|'newer'|'different'|'inconsistent'|'unknown';
const U64_MAX=18446744073709551615n;
function parsed(publication:unknown,collection:string):{incarnation:string;generation:bigint;revision:string}|undefined {
    const envelope=publication!==null&&typeof publication==='object'&&!Array.isArray(publication)?publication as Record<string,unknown>:undefined;
    const token=envelope?envelope.token:publication;
    // Bound work before the regex and BigInt conversion, including hostile saved input.
    if(typeof token!=='string'||token.length>126)return;
    const fields=/^airk-r1:([0-9a-f]{32}):(0|[1-9][0-9]{0,19}):([0-9a-f]{64})$/.exec(token);
    if(!fields||fields[0]!==token)return;
    const generation=BigInt(fields[2]);if(generation>U64_MAX)return;
    if(envelope){
        if(envelope.incarnation!==undefined&&envelope.incarnation!==fields[1])return;
        if(envelope.collection!==undefined&&envelope.collection!==collection)return;
        if(envelope.revision!==undefined&&envelope.revision!==fields[3])return;
        if(envelope.generation!==undefined){
            const supplied=envelope.generation;
            if(typeof supplied==='number'){
                if(!Number.isSafeInteger(supplied)||supplied<0||BigInt(supplied)!==generation)return;
            }else if(typeof supplied!=='string'||supplied!==fields[2])return;
        }
    }
    return {incarnation:fields[1],generation,revision:fields[3]};
}
/** Generation order is only an observation within one authority/collection/incarnation.
 * It proves neither ancestry nor live-source freshness. No input is modified. */
export function comparePublication(result?:PublicationObservation,observed?:PublicationObservation):PublicationRelation {
    if(!result||!observed||typeof result.authority!=='string'||!result.authority||typeof observed.authority!=='string'||!observed.authority
        ||typeof result.collection!=='string'||!result.collection||typeof observed.collection!=='string'||!observed.collection)return 'unknown';
    const a=parsed(result.publication,result.collection),b=parsed(observed.publication,observed.collection);
    if(!a||!b)return 'unknown';
    if(result.authority!==observed.authority||result.collection!==observed.collection||a.incarnation!==b.incarnation)return 'different';
    if(a.generation===b.generation)return a.revision===b.revision?'equal':'inconsistent';
    return a.generation<b.generation?'older':'newer';
}
export function publicationRelationLabel(relation:PublicationRelation):string|undefined {
    switch(relation){
        case 'older':return 'Result generation is older than the observed index generation. This does not establish ancestry or live-source freshness.';
        case 'newer':return 'Result generation is newer than the observed index generation. Readiness remains the separately observed status.';
        case 'different':return 'Result and observed index have different publication scopes; generation order is unavailable.';
        case 'inconsistent':return 'Result and observed index disagree at the same publication generation.';
        default:return;
    }
}
