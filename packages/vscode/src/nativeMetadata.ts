// SPDX-License-Identifier: Apache-2.0
/** Host-only validation and lossless restoration of native metadata headers.
 * Raw transport bodies remain the authoritative copy/export representation. */
import {object,Value,WORKBENCH_MAX_STEP_REPORTS} from './workbenchContracts';
import {framedNativeMetadataDigest,verifySharedCapabilityFields} from './sharedCapabilityFields';
import {materializeCapabilityMember,SHARED_CAPABILITY_REFERENCE} from './workbenchRender';
import {readResultBody} from './sourceVerification';

export const NATIVE_HEADER_REFERENCE='$airk_shared_native_header_fields';
const HEADER_SCHEMA='airk.shared-native-header-templates/1',HEADER_ROOTS=new Set(['collection','discovery_observation','discovery_readiness','semantic_readiness','execution','freshness','generation','publication','request_id','scope_observation','truncated']);
const BAD_KEYS=new Set(['__proto__','constructor','prototype',SHARED_CAPABILITY_REFERENCE,NATIVE_HEADER_REFERENCE]);
const PUBLICATION_PATHS=[['publication'],['read','publication'],['read','current_publication'],['evidence','publication'],['evidence','read','publication'],['evidence','read','current_publication']];
type Primitive=string|number|boolean|null;
type Slot={path:Array<string|number>;kind:'string'|'boolean'|'integer'|'null'};
type Template={fields:Value;slots:Slot[]};
export const nativeHeaderTemplateDigest=(template:Value):string=>framedNativeMetadataDigest('airk/shared-native-header-template/1',template);
const indexId=(raw:unknown):raw is string=>typeof raw==='string'&&/^idx_[0-9a-f]{32}$/.test(raw);
const exact=(value:Value,keys:string)=>Object.keys(value).sort().join(',')===keys;
const plain=(raw:unknown):raw is Value=>!!raw&&typeof raw==='object'&&!Array.isArray(raw)&&[Object.prototype,null].includes(Object.getPrototypeOf(raw));
function bounded(value:unknown,maximum=65536):void {if(Buffer.byteLength(JSON.stringify(value),'utf8')>maximum)throw new Error('Native metadata exceeds its byte budget.');}
function checkHeaderValue(value:unknown,depth=0,work={nodes:0}):void {
    if(depth>32||++work.nodes>32768)throw new Error('Native header exceeds its structural budget.');
    if(value===null||typeof value==='boolean'||typeof value==='number'&&Number.isSafeInteger(value))return;
    if(typeof value==='string'){if(Buffer.byteLength(value,'utf8')>8192)throw new Error('Native header string exceeds its byte budget.');return;}
    if(Array.isArray(value)){if(value.length>1024)throw new Error('Native header array exceeds its bound.');for(const item of value)checkHeaderValue(item,depth+1,work);return;}
    if(!plain(value)||Object.keys(value).length>1024)throw new Error('Invalid native header value.');
    for(const [key,item]of Object.entries(value)){
        if(key.length>128||!/^[\x00-\x7f]*$/.test(key)||BAD_KEYS.has(key))throw new Error('Invalid native header namespace.');
        checkHeaderValue(item,depth+1,work);
    }
}
function target(fields:Value,path:Array<string|number>):{owner:Value|unknown[];key:string|number} {
    let current:unknown=fields;
    for(let i=0;i<path.length;i++){
        const key=path[i];
        if(Array.isArray(current)){
            if(typeof key!=='number'||!Number.isSafeInteger(key)||key<0||key>=current.length)throw new Error('Native header slot does not address an existing value.');
        }else if(!plain(current)||typeof key!=='string'||BAD_KEYS.has(key)||!Object.hasOwn(current,key))throw new Error('Native header slot does not address an existing value.');
        if(i===path.length-1)return {owner:current as Value|unknown[],key};
        current=(current as Record<string|number,unknown>)[key];
    }
    throw new Error('Native header slot path is empty.');
}
function templates(combined:Value):Map<string,Template> {
    const table=object(combined.shared_values);if(!exact(table,'schema,values')||table.schema!==HEADER_SCHEMA||!plain(table.values))throw new Error('Unsupported native header table.');
    const entries=Object.entries(table.values);if(!entries.length||entries.length>32)throw new Error('Invalid native header table count.');bounded(table);
    const result=new Map<string,Template>();
    for(const [id,raw]of entries){
        const row=object(raw);if(!/^[0-9a-f]{64}$/.test(id)||!exact(row,'fields,slots')||!plain(row.fields)||!Object.keys(row.fields).length||Object.keys(row.fields).some(key=>!HEADER_ROOTS.has(key))||!Array.isArray(row.slots)||row.slots.length>512)throw new Error('Invalid native header template.');
        checkHeaderValue(row.fields);const slots:Slot[]=[],paths=new Set<string>();
        for(const item of row.slots){
            const slot=object(item);if(!exact(slot,'kind,path')||!['string','boolean','integer','null'].includes(String(slot.kind))||!Array.isArray(slot.path)||!slot.path.length||slot.path.length>16)throw new Error('Invalid native header slot.');
            const path=slot.path as Array<string|number>;
            if(path.some(key=>typeof key==='string'?!key.length||key.length>128||!/^[\x00-\x7f]*$/.test(key)||BAD_KEYS.has(key):typeof key!=='number'||!Number.isSafeInteger(key)||key<0||key>=1024))throw new Error('Invalid native header slot path.');
            const identity=JSON.stringify(path),leaf=target(row.fields,path);
            if(paths.has(identity)||(leaf.owner as Record<string|number,unknown>)[leaf.key]!==null)throw new Error('Duplicate or non-placeholder native header slot.');
            paths.add(identity);slots.push({path:[...path],kind:slot.kind as Slot['kind']});
        }
        if(nativeHeaderTemplateDigest({fields:row.fields,slots})!==id)throw new Error('Native header template digest mismatch.');
        result.set(id,{fields:row.fields,slots});
    }
    return result;
}
function primitive(value:unknown,kind:Slot['kind']):value is Primitive {
    if(kind==='null')return value===null;
    if(kind==='integer')return typeof value==='number'&&Number.isSafeInteger(value);
    if(kind==='boolean')return typeof value==='boolean';
    return typeof value==='string'&&Buffer.byteLength(value,'utf8')<=8192;
}
function restoreHeader(body:Value,definitions:Map<string,Template>,used:Set<string>):Value {
    if(!Object.hasOwn(body,NATIVE_HEADER_REFERENCE))return body;
    const reference=object(body[NATIVE_HEADER_REFERENCE]);if(!exact(reference,'id,values')||typeof reference.id!=='string'||!Array.isArray(reference.values))throw new Error('Invalid native header reference.');
    const template=definitions.get(reference.id);if(!template||reference.values.length!==template.slots.length)throw new Error('Native header template or values are missing.');
    const fields=structuredClone(template.fields),values=reference.values as unknown[];
    template.slots.forEach((slot,i)=>{
        const value=values[i];if(!primitive(value,slot.kind))throw new Error('Native header slot type mismatch.');
        const leaf=target(fields,slot.path);(leaf.owner as Record<string|number,unknown>)[leaf.key]=value;
    });
    const inline={...body};delete inline[NATIVE_HEADER_REFERENCE];
    if(Object.keys(fields).some(key=>Object.hasOwn(inline,key)))throw new Error('Native header fields collide with inline fields.');
    used.add(reference.id);return {...inline,...fields};
}
function publication(combined:Value,reference:unknown,owner:string):Value {
    const ref=object(reference);if(!exact(ref,'member')||!Number.isSafeInteger(ref.member)||Number(ref.member)<0||Number(ref.member)>=32)throw new Error('Invalid member publication reference.');
    const vector=object(combined.publication_vector),members=Array.isArray(vector.members)?vector.members:[];
    const ids=members.map(row=>object(row).index_id);if(!ids.length||ids.length>32||ids.some((id,i)=>!indexId(id)||(i>0&&String(ids[i-1])>=String(id))))throw new Error('Invalid canonical publication vector members.');
    const member=object(members[Number(ref.member)]),value=object(member.publication);
    if(member.index_id!==owner||!indexId(member.index_id)||!exact(value,'collection,generation,incarnation,revision,token')||typeof value.collection!=='string'||!value.collection||Buffer.byteLength(value.collection,'utf8')>128||typeof value.token!=='string')throw new Error('Member publication reference crosses index scope.');
    const token=/^airk-r1:([0-9a-f]{32}):(0|[1-9][0-9]{0,19}):([0-9a-f]{64})$/.exec(value.token);
    if(!token||value.incarnation!==token[1]||value.revision!==token[3]||!Number.isSafeInteger(value.generation)||Number(value.generation)<0||BigInt(Number(value.generation))!==BigInt(token[2])||BigInt(token[2])>18446744073709551615n)throw new Error('Invalid member publication identity.');
    return value;
}
function restorePublications(body:Value,combined:Value,owner:string):Value {
    let result=body;
    for(const path of PUBLICATION_PATHS){
        let current:unknown=body;for(const key of path){current=plain(current)?current[key]:undefined;}
        if(!plain(current)||!Object.hasOwn(current,'member'))continue;
        if(result===body)result=structuredClone(body);
        let parent=result;for(const key of path.slice(0,-1))parent=object(parent[key]);
        parent[path.at(-1)!]=structuredClone(publication(combined,current,owner));
    }
    return result;
}
function bodyReference(body:Value):boolean {
    if(Object.hasOwn(body,NATIVE_HEADER_REFERENCE)||Object.hasOwn(body,SHARED_CAPABILITY_REFERENCE))return true;
    return PUBLICATION_PATHS.some(path=>{let value:unknown=body;for(const key of path)value=plain(value)?value[key]:undefined;return plain(value)&&Object.hasOwn(value,'member');});
}
function rewriteEnvelope(raw:unknown,body:Value):Value {
    const source=object(raw);
    if(Array.isArray(source.content)&&plain(source.structuredContent)){const result:Value={...source,content:[{type:'text',text:JSON.stringify(body)}],structuredContent:body};delete result.delivery_observation;return result;}
    return body;
}
function hydrateCombined(combined:Value):Value {
    const members=combined.members;if(!Array.isArray(members)||!members.length||members.length>32)throw new Error('Invalid combined native metadata members.');
    const ids=members.map(raw=>object(raw).index_id);if(ids.some(id=>!indexId(id))||new Set(ids).size!==ids.length)throw new Error('Invalid combined native metadata index identities.');
    const schema=object(combined.shared_values).schema;
    const definitions=schema===HEADER_SCHEMA?templates(combined):new Map<string,Template>();
    const capabilities=schema==='airk.shared-capability-fields/1'?verifySharedCapabilityFields(combined):undefined;
    if(combined.shared_values!==undefined&&schema!==HEADER_SCHEMA&&schema!=='airk.shared-capability-fields/1')throw new Error('Unsupported shared native metadata namespace.');
    const used=new Set<string>();let bytes=0;
    const restored=members.map((raw,i)=>{
        const member=object(raw),body=readResultBody(member.result);let result=restoreHeader(body,definitions,used);
        result=materializeCapabilityMember(result,capabilities);result=restorePublications(result,combined,ids[i] as string);
        bounded(result);bytes+=Buffer.byteLength(JSON.stringify(result),'utf8');if(bytes>32*65536)throw new Error('Combined logical metadata exceeds its byte budget.');
        if(result!==body){result={...result};delete result.delivery_observation;}
        return {...member,result:result===body?member.result:rewriteEnvelope(member.result,result)};
    });
    if(definitions.size!==used.size)throw new Error('Unreferenced native header template.');
    const ranking=Array.isArray(combined.ranking)?combined.ranking.map(raw=>{
        const rank=object(raw),origin=object(rank.origin);if(!Object.hasOwn(origin,'publication_ref'))return raw;
        if(!indexId(rank.index_id)||origin.index_id!==rank.index_id||!ids.includes(rank.index_id)||!exact(origin,'index_id,project_ids,publication_ref,source_relative_path,source_root_id')||typeof origin.source_root_id!=='string'||!/^src_[0-9a-f]{32}$/.test(origin.source_root_id)||!Array.isArray(origin.project_ids)||!origin.project_ids.length||origin.project_ids.length>32||new Set(origin.project_ids).size!==origin.project_ids.length||origin.project_ids.some(id=>typeof id!=='string'||!/^prj_[0-9a-f]{32}$/.test(id))||typeof origin.source_relative_path!=='string'||!origin.source_relative_path||origin.source_relative_path.startsWith('/')||/[\\:\u0000]/.test(origin.source_relative_path)||Buffer.byteLength(origin.source_relative_path,'utf8')>4096||origin.source_relative_path.split('/').some(part=>!part||part==='.'||part==='..'))throw new Error('Ranked publication reference crosses index scope.');
        const restored:Value={...origin,publication:structuredClone(publication(combined,origin.publication_ref,rank.index_id))};delete restored.publication_ref;
        return {...rank,origin:restored};
    }):combined.ranking;
    const result:Value={...combined,derived_view:"materialized_native_members",members:restored,...(ranking!==undefined?{ranking}:{})};delete result.delivery_observation;return result;
}
function visit(raw:Value,mode:'detect'|'hydrate',depth=0):Value|boolean {
    if(depth>8)throw new Error('Native metadata nesting exceeds its bound.');
    const body=readResultBody(raw);
    if(body.schema==='airk.combined-read/1'){
        const detected=body.shared_values!==undefined||(Array.isArray(body.members)&&body.members.some(row=>bodyReference(readResultBody(object(row).result))))
            ||(Array.isArray(body.ranking)&&body.ranking.some(row=>Object.hasOwn(object(object(row).origin),'publication_ref')));
        if(mode==='detect')return detected;
        return detected?rewriteEnvelope(raw,hydrateCombined(body)):raw;
    }
    let changed=false;const result={...body};
    if(Array.isArray(body.steps)){
        if(body.steps.length>WORKBENCH_MAX_STEP_REPORTS)throw new Error('Native metadata step reports exceed the canonical bound.');
        result.steps=body.steps.map(item=>{const row=object(item);if(!plain(row.result))return item;const child=visit(row.result,mode,depth+1);if(mode==='detect'){changed||=child===true;return item;}if(child!==row.result){changed=true;return {...row,result:child};}return item;});
    }
    if(plain(body.member_verification)){
        const check=body.member_verification;
        if(check.schema==='airk.combined-verification/1'&&Array.isArray(check.members)){
            result.member_verification={...check,members:check.members.map(item=>{const row=object(item);if(!plain(row.result))return item;const child=visit(row.result,mode,depth+1);if(mode==='detect'){changed||=child===true;return item;}if(child!==row.result){changed=true;return {...row,result:child};}return item;})};
        }
    }
    if(mode==='detect')return changed;
    if(!changed)return raw;delete result.delivery_observation;return rewriteEnvelope(raw,result);
}
export function hasNativeMetadataReferences(value:unknown):boolean {return plain(value)&&visit(value,'detect')===true;}
/** Restore a derived logical view; callers must retain raw for receipt checks,
 * persistence and full copies. Native admission validated the raw response. */
export function materializeNativeMetadata(raw:Value):Value {return visit(raw,'hydrate') as Value;}

/** Object identity binds each proof to its exact native frame/session. This
 * weak cache never turns a saved observation into current read authority. */
export class NativeMetadataProjectionCache {
    private views=new WeakMap<Value,{payload?:Value;error?:string}>();
    remember(raw:Value,payload:Value):void{this.views.set(raw,{payload});}
    view(raw:Value):{payload?:Value;error?:string} {
        const saved=this.views.get(raw);if(saved)return saved;
        let result:{payload?:Value;error?:string};
        try{result={payload:hasNativeMetadataReferences(raw)?materializeNativeMetadata(raw):raw};}
        catch{result={error:'Native metadata could not be restored. The complete original response remains available.'};}
        this.views.set(raw,result);return result;
    }
    clear():void{this.views=new WeakMap();}
}
