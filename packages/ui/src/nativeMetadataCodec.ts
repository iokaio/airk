// SPDX-License-Identifier: Apache-2.0
/** Single browser-safe lossless native metadata owner. Hashing is injected; raw native responses remain authoritative. */
export type Value=Record<string,unknown>;
export type CapabilityFieldDisplay={state:'verified';values:Record<string,Value>}|{state:'unavailable';error:string};
export type ProjectionTurn={id:string;envelope?:Value};
// Moved unchanged from the existing Workbench UI owners, not new native limits.
export const WORKBENCH_MAX_STEP_REPORTS=72;
export const DISPLAY_PROJECTION_TURNS=200;
export const NATIVE_HEADER_REFERENCE='$airk_shared_native_header_fields';
const object=(v:unknown):Value=>v&&typeof v==='object'&&!Array.isArray(v)?v as Value:{};
const utf8=(value:string):Uint8Array=>{if(typeof value!=='string')throw new TypeError('Native metadata UTF8 input must be a string.');return new TextEncoder().encode(value);};
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

const MAX_BYTES=65536,UNSAFE_KEYS=new Set(['__proto__','constructor','prototype']);
function canonicalJson(value:unknown,depth=0,work={nodes:0}):string {
    if(depth>32||++work.nodes>32768)throw new Error('Shared capability value exceeds its structural budget.');
    if(value===null||typeof value==='boolean')return JSON.stringify(value);
    if(typeof value==='string'){
        if(/[\uD800-\uDBFF](?![\uDC00-\uDFFF])|(?<![\uD800-\uDBFF])[\uDC00-\uDFFF]/u.test(value))throw new Error('Invalid shared capability Unicode.');
        return JSON.stringify(value);
    }
    if(typeof value==='number'&&Number.isSafeInteger(value))return JSON.stringify(value);
    if(Array.isArray(value))return '['+value.map(item=>canonicalJson(item,depth+1,work)).join(',')+']';
    if(!value||typeof value!=='object'||![Object.prototype,null].includes(Object.getPrototypeOf(value))||Object.getOwnPropertySymbols(value).length)throw new Error('Invalid shared capability value.');
    // Object construction followed by JSON.stringify would reorder integer-like
    // keys numerically. Native maps order every ASCII key lexically instead.
    return '{'+Object.keys(value).sort().map(key=>{
        if(!/^[\x00-\x7f]*$/.test(key)||UNSAFE_KEYS.has(key)||key===SHARED_CAPABILITY_REFERENCE)throw new Error('Invalid shared capability namespace.');
        return JSON.stringify(key)+':'+canonicalJson((value as Value)[key],depth+1,work);
    }).join(',')+'}';
}

/** Exactly the prior native framed digest input; this function performs no cryptography. */
export function framedNativeMetadataBytes(domain:string,value:Value):Uint8Array {
    const bytes=utf8(canonicalJson(value));
    if(bytes.length>MAX_BYTES)throw new Error('Shared capability fields exceed their byte budget.');
    const fields=[utf8(domain),bytes],magic=utf8('AIRK-DIGEST-1\0');
    const framed=new Uint8Array(magic.length+fields.reduce((total,field)=>total+8+field.length,0));
    framed.set(magic);let offset=magic.length;
    for(const field of fields){new DataView(framed.buffer,framed.byteOffset+offset,8).setBigUint64(0,BigInt(field.length),false);offset+=8;framed.set(field,offset);offset+=field.length;}
    return framed;
}
export function createNativeMetadataCodec(sha256:(framedBytes:Uint8Array)=>string) {
    function framedNativeMetadataDigest(domain:string,value:Value):string{return sha256(framedNativeMetadataBytes(domain,value));}
    function sharedCapabilityDigest(fields:Value):string{return framedNativeMetadataDigest('airk/shared-capability-fields/1',fields);}
function checkReferences(value:unknown,allowRoot=false,depth=0,work={nodes:0}):void {
    if(depth>32||++work.nodes>32768)throw new Error('Shared capability reference exceeds its structural budget.');
    if(Array.isArray(value)){for(const item of value)checkReferences(item,false,depth+1,work);return;}
    if(!value||typeof value!=='object')return;
    for(const [key,item]of Object.entries(value)){
        if(key===SHARED_CAPABILITY_REFERENCE&&!allowRoot)throw new Error('Nested shared capability reference.');
        if(key!==SHARED_CAPABILITY_REFERENCE)checkReferences(item,false,depth+1,work);
    }
}
function verifySharedCapabilityFields(combined:Value):CapabilityFieldDisplay|undefined {
    const members=Array.isArray(combined.members)?combined.members:[];
    const referenced=members.some(raw=>Object.hasOwn(readResultBody(object(raw).result),SHARED_CAPABILITY_REFERENCE));
    if(combined.shared_values===undefined&&!referenced)return;
    if(combined.schema!=='airk.combined-read/1'||!members.length||members.length>32)throw new Error('Invalid combined capability result.');
    const indexes=members.map(raw=>object(raw).index_id);
    if(indexes.some(id=>typeof id!=='string'||!/^idx_[0-9a-f]{32}$/.test(id))||new Set(indexes).size!==indexes.length)throw new Error('Invalid or duplicate combined capability member identity.');
    const table=object(combined.shared_values),values=object(table.values);
    if(Object.keys(table).sort().join(',')!=='schema,values'||table.schema!=='airk.shared-capability-fields/1'
        ||!Object.keys(values).length||Object.keys(values).length>32||utf8(JSON.stringify(table)).length>MAX_BYTES)throw new Error('Invalid shared capability table.');
    const verified:Record<string,Value>={};
    for(const [id,raw]of Object.entries(values)){
        const fields=object(raw);
        if(!/^[0-9a-f]{64}$/.test(id)||!Object.keys(fields).length||Object.keys(fields).some(key=>!shareableCapabilityField(key))||sharedCapabilityDigest(fields)!==id)throw new Error('Shared capability digest or namespace mismatch.');
        verified[id]=structuredClone(fields);
    }
    const display:CapabilityFieldDisplay={state:'verified',values:verified},used=new Set<string>();let expandedBytes=0;
    for(const raw of members){
        const body=readResultBody(object(raw).result);checkReferences(body,true);
        let materialized=body;
        if(Object.hasOwn(body,SHARED_CAPABILITY_REFERENCE)){
            const id=body[SHARED_CAPABILITY_REFERENCE];if(typeof id!=='string')throw new Error('Invalid shared capability reference.');
            materialized=materializeCapabilityMember(body,display);
            used.add(id);
        }
        const bytes=utf8(JSON.stringify(materialized)).length;expandedBytes+=bytes;
        if(bytes>MAX_BYTES||expandedBytes>32*MAX_BYTES)throw new Error('Materialized capability display exceeds its byte budget.');
    }
    if(used.size!==Object.keys(verified).length)throw new Error('Unreferenced shared capability value.');
    return display;
}
const HEADER_SCHEMA='airk.shared-native-header-templates/1',HEADER_ROOTS=new Set(['collection','discovery_observation','discovery_readiness','semantic_readiness','execution','freshness','generation','publication','request_id','scope_observation','truncated']);
const BAD_KEYS=new Set(['__proto__','constructor','prototype',SHARED_CAPABILITY_REFERENCE,NATIVE_HEADER_REFERENCE]);
const PUBLICATION_PATHS=[['publication'],['read','publication'],['read','current_publication'],['evidence','publication'],['evidence','read','publication'],['evidence','read','current_publication']];
type Primitive=string|number|boolean|null;
type Slot={path:Array<string|number>;kind:'string'|'boolean'|'integer'|'null'};
type Template={fields:Value;slots:Slot[]};
const nativeHeaderTemplateDigest=(template:Value):string=>framedNativeMetadataDigest('airk/shared-native-header-template/1',template);
const indexId=(raw:unknown):raw is string=>typeof raw==='string'&&/^idx_[0-9a-f]{32}$/.test(raw);
const exact=(value:Value,keys:string)=>Object.keys(value).sort().join(',')===keys;
const plain=(raw:unknown):raw is Value=>!!raw&&typeof raw==='object'&&!Array.isArray(raw)&&[Object.prototype,null].includes(Object.getPrototypeOf(raw));
function bounded(value:unknown,maximum=65536):void {if(utf8(JSON.stringify(value)).length>maximum)throw new Error('Native metadata exceeds its byte budget.');}
function checkHeaderValue(value:unknown,depth=0,work={nodes:0}):void {
    if(depth>32||++work.nodes>32768)throw new Error('Native header exceeds its structural budget.');
    if(value===null||typeof value==='boolean'||typeof value==='number'&&Number.isSafeInteger(value))return;
    if(typeof value==='string'){if(utf8(value).length>8192)throw new Error('Native header string exceeds its byte budget.');return;}
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
    return typeof value==='string'&&utf8(value).length<=8192;
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
    if(member.index_id!==owner||!indexId(member.index_id)||!exact(value,'collection,generation,incarnation,revision,token')||typeof value.collection!=='string'||!value.collection||utf8(value.collection).length>128||typeof value.token!=='string')throw new Error('Member publication reference crosses index scope.');
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
        bounded(result);bytes+=utf8(JSON.stringify(result)).length;if(bytes>32*65536)throw new Error('Combined logical metadata exceeds its byte budget.');
        if(result!==body){result={...result};delete result.delivery_observation;}
        return {...member,result:result===body?member.result:rewriteEnvelope(member.result,result)};
    });
    if(definitions.size!==used.size)throw new Error('Unreferenced native header template.');
    const ranking=Array.isArray(combined.ranking)?combined.ranking.map(raw=>{
        const rank=object(raw),origin=object(rank.origin);if(!Object.hasOwn(origin,'publication_ref'))return raw;
        if(!indexId(rank.index_id)||origin.index_id!==rank.index_id||!ids.includes(rank.index_id)||!exact(origin,'index_id,project_ids,publication_ref,source_relative_path,source_root_id')||typeof origin.source_root_id!=='string'||!/^src_[0-9a-f]{32}$/.test(origin.source_root_id)||!Array.isArray(origin.project_ids)||origin.project_ids.length>32||new Set(origin.project_ids).size!==origin.project_ids.length||origin.project_ids.some(id=>typeof id!=='string'||!/^prj_[0-9a-f]{32}$/.test(id))||typeof origin.source_relative_path!=='string'||!origin.source_relative_path||origin.source_relative_path.startsWith('/')||/[\\:\u0000]/.test(origin.source_relative_path)||utf8(origin.source_relative_path).length>4096||origin.source_relative_path.split('/').some(part=>!part||part==='.'||part==='..'))throw new Error('Ranked publication reference crosses index scope.');
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
function hasNativeMetadataReferences(value:unknown):boolean {return plain(value)&&visit(value,'detect')===true;}
/** Restore a derived logical view; callers must retain raw for receipt checks,
 * persistence and full copies. Native admission validated the raw response. */
function materializeNativeMetadata(raw:Value):Value {return visit(raw,'hydrate') as Value;}

    return {framedNativeMetadataDigest,sharedCapabilityDigest,verifySharedCapabilityFields,nativeHeaderTemplateDigest,hasNativeMetadataReferences,materializeNativeMetadata};
}
export type NativeMetadataCodec=ReturnType<typeof createNativeMetadataCodec>;
export class NativeMetadataProjectionCacheBase {
    constructor(private readonly codec:NativeMetadataCodec){}
    private views=new WeakMap<Value,{payload?:Value;error?:string}>();
    remember(raw:Value,payload:Value):void{this.views.set(raw,{payload});}
    view(raw:Value):{payload?:Value;error?:string} {
        const saved=this.views.get(raw);if(saved)return saved;
        let result:{payload?:Value;error?:string};
        try{result={payload:this.codec.hasNativeMetadataReferences(raw)?this.codec.materializeNativeMetadata(raw):raw};}
        catch{result={error:'Native metadata could not be restored. The complete original response remains available.'};}
        this.views.set(raw,result);return result;
    }
    clear():void{this.views=new WeakMap();}
}

export class SharedCapabilityDisplayCacheBase {
    constructor(private readonly codec:NativeMetadataCodec,private readonly combinedReadDetails:(value:Value)=>Array<{step:string;result:Value}>){}
    private readonly retained=new Map<ProjectionTurn,{envelope:Value|undefined;details:Record<string,CapabilityFieldDisplay>}>();
    project(turns:ProjectionTurn[]):Record<string,Record<string,CapabilityFieldDisplay>> {
        const current=new Set(turns.slice(-DISPLAY_PROJECTION_TURNS));for(const turn of this.retained.keys())if(!current.has(turn))this.retained.delete(turn);
        return Object.fromEntries(turns.map(turn=>{
            let cached=this.retained.get(turn);
            if(!cached||cached.envelope!==turn.envelope){
                const details:Record<string,CapabilityFieldDisplay>=Object.create(null);
                for(const detail of this.combinedReadDetails(turn.envelope??{})){
                    if(object(detail.result.shared_values).schema==='airk.shared-native-header-templates/1')continue;
                    try{const display=this.codec.verifySharedCapabilityFields(detail.result);if(display)details[detail.step]=display;}
                    catch{details[detail.step]={state:'unavailable',error:'Shared capability data could not be validated. The complete original response remains available.'};}
                }
                cached={envelope:turn.envelope,details};if(current.has(turn))this.retained.set(turn,cached);
            }
            return [turn.id,cached.details];
        }));
    }
    clear():void{this.retained.clear();}
}

/** Full raw JSON graph equality for proof custody, without applying metadata canonical rules to source data. */
function sameRawGraph(left:unknown,right:unknown,seen=new WeakMap<object,object>()):boolean {
    if(Object.is(left,right))return true;
    if(left===null||right===null||typeof left!=='object'||typeof right!=='object')return false;
    const prior=seen.get(left);if(prior!==undefined)return prior===right;seen.set(left,right);
    const arrays=Array.isArray(left)||Array.isArray(right);
    if(arrays&&(!Array.isArray(left)||!Array.isArray(right)||left.length!==right.length))return false;
    const plain=(value:object)=>[Object.prototype,null].includes(Object.getPrototypeOf(value));
    if(!arrays&&(!plain(left)||!plain(right)))return false;
    const keys=Reflect.ownKeys(left),other=Reflect.ownKeys(right);
    return keys.length===other.length&&keys.every(key=>Object.hasOwn(right,key)&&sameRawGraph((left as Record<PropertyKey,unknown>)[key],(right as Record<PropertyKey,unknown>)[key],seen));
}
/** Internal suspended-digest request; never a partially verified result. */
class MissingDigest extends Error {constructor(readonly bytes:Uint8Array){super('Native metadata digest required.');}}
/** Async WebCrypto shell over the exact same synchronous validator, with no alternate acceptance logic. */
export function createBrowserNativeMetadataCodec(subtle:Pick<SubtleCrypto,'digest'>=globalThis.crypto.subtle) {
    type Proof={owner:NativeMetadataCodec;snapshot:Value;checkCurrent:()=>void};
    const verified=new WeakMap<Value,Proof>();
    const key=(bytes:Uint8Array)=>Array.from(bytes,byte=>byte.toString(16).padStart(2,'0')).join('');
    const detector=createNativeMetadataCodec(()=>{throw new Error('Native metadata digest has not been verified.');});
    const unchanged=(raw:Value,proof:Proof)=>{proof.checkCurrent();if(!sameRawGraph(raw,proof.snapshot))throw new Error('Native metadata raw response changed after validation.');};
    const codec={hasNativeMetadataReferences:detector.hasNativeMetadataReferences,materializeNativeMetadata:(raw:Value):Value=>{
        const proof=verified.get(raw);if(!proof)throw new Error('Native metadata has not been verified for this exact response.');
        unchanged(raw,proof);const result=proof.owner.materializeNativeMetadata(raw);unchanged(raw,proof);return result;
    }};
    async function materializeNativeMetadata(raw:Value,checkCurrent:()=>void=()=>undefined):Promise<Value>{
        checkCurrent();const prior=verified.get(raw);
        // Reuse retains the original scope custodian; caller checks apply only to this call, never a recursive proof chain.
        if(prior){unchanged(raw,prior);const result=prior.owner.materializeNativeMetadata(raw);checkCurrent();unchanged(raw,prior);return result;}
        const snapshot=structuredClone(raw);
        const current=()=>{checkCurrent();if(!sameRawGraph(raw,snapshot))throw new Error('Native metadata raw response changed during validation.');};
        const digests=new Map<string,string>();let retainedBytes=0;
        const owner=createNativeMetadataCodec(bytes=>{const found=digests.get(key(bytes));if(found!==undefined)return found;throw new MissingDigest(bytes.slice());});
        // Same existing structural work cap, table cap and canonical report bound; no resource-policy override.
        for(let work=0;work<=32768;work++){
            current();
            try{const result=owner.materializeNativeMetadata(raw);current();verified.set(raw,{owner,snapshot,checkCurrent});return result;}
            catch(error){
                if(!(error instanceof MissingDigest))throw error;
                const identity=key(error.bytes);if(digests.has(identity))throw new Error('Native metadata digest made no progress.');
                if(digests.size>=32768||retainedBytes+error.bytes.length>WORKBENCH_MAX_STEP_REPORTS*32*65536)throw new Error('Native metadata digest cache exceeds its structural budget.');
                current();const actual=await subtle.digest('SHA-256',error.bytes.slice().buffer);current();
                const value=Array.from(new Uint8Array(actual),byte=>byte.toString(16).padStart(2,'0')).join('');
                if(!/^[0-9a-f]{64}$/.test(value))throw new Error('Native metadata digest provider returned an invalid SHA-256.');
                digests.set(identity,value);retainedBytes+=error.bytes.length;
            }
        }
        throw new Error('Native metadata digest work exceeds its structural budget.');
    }
    return {codec,materializeNativeMetadata};
}
