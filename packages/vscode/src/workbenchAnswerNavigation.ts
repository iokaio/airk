// SPDX-License-Identifier: Apache-2.0
// Host projection of existing native receipts. It grants no live read authority.
import type {NativeMetadataCodec} from './nativeMetadataCodec';
import type { ContextResponseV3 } from './generated/arbiter';
import { object, type Value } from './workbenchContracts';
import { nativeVerification, readResultBody } from './sourceVerification';
import { comparePublication } from './publications';
const digest = (v: unknown): v is string => typeof v === 'string' && /^[0-9a-f]{64}$/.test(v);
const indexId = (v: unknown): v is string => typeof v === 'string' && /^idx_[0-9a-f]{32}$/.test(v);
const projectIds = (v: unknown): v is string[] => Array.isArray(v)
    && new Set(v).size === v.length && v.every(id => typeof id === 'string' && /^prj_[0-9a-f]{32}$/.test(id));
const equalPublication = (a: unknown, b: unknown, collection: unknown): boolean => typeof collection === 'string'
    && comparePublication({ authority: 'native-answer', collection, publication: a }, { authority: 'native-answer', collection, publication: b }) === 'equal';

export const ANSWER_SCHEMA='airk.arbiter.context/3', COMBINED_ANSWER_SCHEMA='airk.combined-context/2';
/** Display projection only. The full original outer result remains the export owner. */
export interface AnswerPartition { member?: number; indexId?: string; outcome: string; packet?: ContextResponseV3; raw: Value }
function contextPacket(raw: unknown): ContextResponseV3 | undefined {
    const body=readResultBody(raw),scope=object(body.scope);
    if(body.schema!==ANSWER_SCHEMA||typeof body.question!=='string'||!Array.isArray(body.records)||!Array.isArray(body.parts)||!Array.isArray(body.paragraphs)
        ||!Array.isArray(body.decisions)||!Array.isArray(body.degradations)||!Array.isArray(body.record_uses)||!Array.isArray(body.omitted_evidence)
        ||!Object.keys(object(body.decision_log)).length||!Object.keys(object(body.union)).length||!indexId(scope.index_id)||!Object.keys(object(scope.publication)).length)return;
    return body as unknown as ContextResponseV3;
}
/** Answer /3, alone or as the members of one combined Answer. /2 replies are history: shown whole, never navigated. */
export function isAnswerEnvelope(envelope:Value):boolean {
    return envelope.schema===ANSWER_SCHEMA||envelope.schema==='airk.combined-read/1'&&object(envelope.context).schema===COMBINED_ANSWER_SCHEMA;
}
export function answerPartitions(envelope:Value):AnswerPartition[] {
    if(envelope.schema===ANSWER_SCHEMA)return [{outcome:'local',packet:contextPacket(envelope),raw:envelope}];
    if(!isAnswerEnvelope(envelope)||!Array.isArray(envelope.members))return [];
    const members=envelope.members.map(object),ids=members.map(member=>member.index_id);
    const vector=object(envelope.publication_vector),publications=Array.isArray(vector.members)?vector.members.map(object):[];
    return members.map((raw,member)=>{
        const packet=contextPacket(raw.result),index=raw.index_id;
        const heads=publications.filter(head=>head.index_id===index),scope=object(packet?.scope),publication=object(scope.publication);
        const bound=indexId(index)&&ids.filter(id=>id===index).length===1&&scope.index_id===index&&heads.length===1
            &&equalPublication(scope.publication,heads[0].publication,publication.collection);
        return {member,indexId:typeof index==='string'?index:undefined,outcome:String(raw.outcome??'unknown'),
            ...(bound?{packet}:{}),raw};
    });
}

/** Follow only addressable JSON objects in the original receipt custodian.
 * Parsing an error's text for display never creates a source locator. */
function sourceAtPointer(root:unknown,pointer:string):Value|undefined {
    if(!pointer.startsWith('/'))return;
    let current:unknown=root;
    for(const token of pointer.slice(1).split('/')){
        if(/~[^01]|~$/.test(token))return;
        const key=token.replaceAll('~1','/').replaceAll('~0','~');
        if(Array.isArray(current)){
            if(!/^(0|[1-9][0-9]*)$/.test(key)||Number(key)>=current.length)return;
            current=current[Number(key)];
        }else{
            const row=object(current);if(!Object.hasOwn(row,key))return;current=row[key];
        }
    }
    return current!==null&&typeof current==='object'&&!Array.isArray(current)?current as Value:undefined;
}

/** Resolve an exact returned-reference association; roots never come from the renderer. */
export function answerSourceBinding(body: Value, path: string, hash: string, index: string, publication: unknown,
    pointer?: string, receiptOwner: Value = body): Value | undefined {
    const receipt = nativeVerification(receiptOwner.source_verification);
    if (!receipt) return;
    if(pointer!==undefined){
        const actual=sourceAtPointer(receiptOwner,pointer);
        if(actual?.path!==path||actual.content_hash!==hash)return;
    }
    const references = (receipt.references as unknown[]).map(object);
    const sources = (receipt.sources as unknown[]).map(object);
    const origins = (receipt.origins as unknown[]).map(object);
    if (new Set(sources.map(source => source.id)).size !== sources.length
        || new Set(origins.map(origin => origin.id)).size !== origins.length
        || new Set(references.map(reference => reference.id)).size !== references.length
        || !equalPublication(body.publication, publication, body.collection)) return;
    const matching = sources.filter(source => source.path === path && source.expected_hash === hash
        && typeof source.source_root_id === 'string' && /^src_[0-9a-f]{32}$/.test(source.source_root_id)
        && typeof source.source_relative_path === 'string' && source.source_relative_path.length > 0
        && !/[\\:\u0000]/.test(source.source_relative_path)
        && source.source_relative_path.split('/').every(part => !['', '.', '..'].includes(part))
        && origins.filter(origin => origin.id === source.origin_id && origin.index_id === index
            && equalPublication(origin.publication, publication, body.collection)
            && projectIds(origin.project_ids)).length === 1
        && (pointer === undefined || references.filter(reference => reference.result_pointer === pointer
            && reference.source_id === source.id && Array.isArray(source.reference_ids) && source.reference_ids.includes(reference.id)).length === 1));
    if (matching.length !== 1) return;
    const source = matching[0], origin = origins.find(origin => origin.id === source.origin_id)!;
    return { index_id: index, project_ids: origin.project_ids, source_root_id: source.source_root_id,
        source_relative_path: source.source_relative_path, publication };
}

/** Flatten every located span of each record, preserving record ID and span locator. */
export function answerEvidenceRows(envelope: Value): Value[] { return answerEvidenceRowsAt(envelope); }
type ReceiptCustody = {owner:Value;prefix:string};
function structuredBodyPrefix(raw:unknown):string|undefined {
    const value=object(raw);
    if(Array.isArray(value.content)&&Object.keys(object(value.structuredContent)).length)return '/structuredContent';
    return readResultBody(raw)===value?'':undefined;
}
function answerEvidenceRowsAt(envelope:Value,inherited?:ReceiptCustody):Value[] {
    const custody=inherited??(Object.hasOwn(envelope,'source_verification')?{owner:envelope,prefix:''}:undefined);
    if(envelope.schema==='airk.combined-read/1')return answerPartitions(envelope).flatMap(partition=>{
        if(!partition.packet||partition.outcome!=='complete'||partition.member===undefined)return [];
        const result=partition.raw.result,prefix=structuredBodyPrefix(result);
        if(prefix===undefined)return [];
        const child=custody?{owner:custody.owner,prefix:`${custody.prefix}/members/${partition.member}/result${prefix}`}:undefined;
        return answerEvidenceRowsAt(partition.packet as unknown as Value,child).map(row=>({...row,answer_member:partition.member}));
    });
    const packet=contextPacket(envelope);
    if(!packet)return [];
    const scope=packet.scope as unknown as Value,publication=object(scope.publication),ids=packet.records.map(record=>record.id);
    if (ids.some(id => !digest(id)) || new Set(ids).size !== ids.length) return [];
    // The reply carries one scope and one whole-reply verification receipt;
    // each record is a span projection whose spans are the receipt's locators.
    const owner=custody?.owner??envelope,prefix=custody?.prefix??'',fence={publication:scope.publication,collection:publication.collection};
    return packet.records.flatMap((record, recordPosition) => {
        const body=object(record.canonical_result);
        if(body.revision!=null&&body.revision!==publication.token)return [];
        return (Array.isArray(body.spans)?body.spans:[]).flatMap((raw, span) => {
            const row = object(raw);
            if (typeof row.path !== 'string' || !digest(row.content_hash) || !Number.isSafeInteger(row.start_line) || !Number.isSafeInteger(row.end_line)) return [];
            const origin = answerSourceBinding(fence, row.path, row.content_hash, String(scope.index_id), scope.publication,
                `${prefix}/records/${recordPosition}/canonical_result/spans/${span}`, owner);
            if (!origin) return [];
            return [{ citation:{path:row.path,content_hash:row.content_hash,start_line:row.start_line,end_line:row.end_line},
                ...(typeof row.text==='string'?{text:row.text}:{}), span:row, origin, publication: publication.token, step: record.id, result_pointer: `/spans/${span}` }];
        });
    });
}

/** Browser/host callers may inject the same verified metadata owner before any navigation projection. */
export function createAnswerNavigation(codec?:Pick<NativeMetadataCodec,'hasNativeMetadataReferences'|'materializeNativeMetadata'>) {
    const logical=(raw:Value)=>codec?codec.materializeNativeMetadata(raw):raw;
    return {answerPartitions:(raw:Value)=>answerPartitions(logical(raw)),answerEvidenceRows:(raw:Value)=>answerEvidenceRows(logical(raw))};
}
