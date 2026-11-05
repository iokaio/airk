// SPDX-License-Identifier: Apache-2.0
/** Host-only digest validation for complete shared native capability fields. */
import { createHash } from 'node:crypto';
import { CapabilityFieldDisplay, object, Value, WorkbenchTurn } from './workbenchContracts';
import { combinedReadDetails, DISPLAY_PROJECTION_TURNS, materializeCapabilityMember, shareableCapabilityField, SHARED_CAPABILITY_REFERENCE } from './workbenchRender';
import { readResultBody } from './sourceVerification';

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
export function sharedCapabilityDigest(fields:Value):string {
    return framedNativeMetadataDigest('airk/shared-capability-fields/1',fields);
}
/** Shared exact canonical digest for versioned native metadata representations. */
export function framedNativeMetadataDigest(domain:string,value:Value):string {
    const bytes=Buffer.from(canonicalJson(value),'utf8');
    if(bytes.length>MAX_BYTES)throw new Error('Shared capability fields exceed their byte budget.');
    const digest=createHash('sha256');digest.update('AIRK-DIGEST-1\0','utf8');
    for(const field of [Buffer.from(domain,'utf8'),bytes]){
        const length=Buffer.alloc(8);length.writeBigUInt64BE(BigInt(field.length));digest.update(length);digest.update(field);
    }
    return digest.digest('hex');
}
function checkReferences(value:unknown,allowRoot=false,depth=0,work={nodes:0}):void {
    if(depth>32||++work.nodes>32768)throw new Error('Shared capability reference exceeds its structural budget.');
    if(Array.isArray(value)){for(const item of value)checkReferences(item,false,depth+1,work);return;}
    if(!value||typeof value!=='object')return;
    for(const [key,item]of Object.entries(value)){
        if(key===SHARED_CAPABILITY_REFERENCE&&!allowRoot)throw new Error('Nested shared capability reference.');
        if(key!==SHARED_CAPABILITY_REFERENCE)checkReferences(item,false,depth+1,work);
    }
}
export function verifySharedCapabilityFields(combined:Value):CapabilityFieldDisplay|undefined {
    const members=Array.isArray(combined.members)?combined.members:[];
    const referenced=members.some(raw=>Object.hasOwn(readResultBody(object(raw).result),SHARED_CAPABILITY_REFERENCE));
    if(combined.shared_values===undefined&&!referenced)return;
    if(combined.schema!=='airk.combined-read/1'||!members.length||members.length>32)throw new Error('Invalid combined capability result.');
    const indexes=members.map(raw=>object(raw).index_id);
    if(indexes.some(id=>typeof id!=='string'||!/^idx_[0-9a-f]{32}$/.test(id))||new Set(indexes).size!==indexes.length)throw new Error('Invalid or duplicate combined capability member identity.');
    const table=object(combined.shared_values),values=object(table.values);
    if(Object.keys(table).sort().join(',')!=='schema,values'||table.schema!=='airk.shared-capability-fields/1'
        ||!Object.keys(values).length||Object.keys(values).length>32||Buffer.byteLength(JSON.stringify(table),'utf8')>MAX_BYTES)throw new Error('Invalid shared capability table.');
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
        const bytes=Buffer.byteLength(JSON.stringify(materialized),'utf8');expandedBytes+=bytes;
        if(bytes>MAX_BYTES||expandedBytes>32*MAX_BYTES)throw new Error('Materialized capability display exceeds its byte budget.');
    }
    if(used.size!==Object.keys(verified).length)throw new Error('Unreferenced shared capability value.');
    return display;
}
/** Cache only one validated table per native combined result, never expanded
 * copies per member; opening details materializes the chosen body on demand. */
export class SharedCapabilityDisplayCache {
    private readonly retained=new Map<WorkbenchTurn,{envelope:Value|undefined;details:Record<string,CapabilityFieldDisplay>}>();
    project(turns:WorkbenchTurn[]):Record<string,Record<string,CapabilityFieldDisplay>> {
        const current=new Set(turns.slice(-DISPLAY_PROJECTION_TURNS));for(const turn of this.retained.keys())if(!current.has(turn))this.retained.delete(turn);
        return Object.fromEntries(turns.map(turn=>{
            let cached=this.retained.get(turn);
            if(!cached||cached.envelope!==turn.envelope){
                const details:Record<string,CapabilityFieldDisplay>=Object.create(null);
                for(const detail of combinedReadDetails(turn.envelope??{})){
                    if(object(detail.result.shared_values).schema==='airk.shared-native-header-templates/1')continue;
                    try{const display=verifySharedCapabilityFields(detail.result);if(display)details[detail.step]=display;}
                    catch{details[detail.step]={state:'unavailable',error:'Shared capability data could not be validated. The complete original response remains available.'};}
                }
                cached={envelope:turn.envelope,details};if(current.has(turn))this.retained.set(turn,cached);
            }
            return [turn.id,cached.details];
        }));
    }
    clear():void{this.retained.clear();}
}
