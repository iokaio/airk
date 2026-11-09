// SPDX-License-Identifier: Apache-2.0
import { identifier, record, RecordValue, ServiceError } from './serviceClient';

export const RUNTIME_UPGRADE_CHOICES = [
    {label:'Migrate saved indexes',description:'Keep the existing indexes and their saved configuration',mode:'migrate' as const},
    {label:'Rebuild indexes from saved sources',description:'Preserve old state and create fresh indexes with saved scope and settings',mode:'rebuild' as const},
];
export async function chooseRuntimeUpgradeMode(
    pick:(items:typeof RUNTIME_UPGRADE_CHOICES)=>PromiseLike<(typeof RUNTIME_UPGRADE_CHOICES)[number]|undefined>,
):Promise<'migrate'|'rebuild'|undefined>{
    const choice=await pick(RUNTIME_UPGRADE_CHOICES);
    if(choice===undefined)return undefined;
    if(!RUNTIME_UPGRADE_CHOICES.includes(choice))throw new ServiceError('invalid_runtime_rebuild_choice');
    return choice.mode;
}

export type NativeRebuildObservation=RecordValue & {schema:1;operation_id:string;stage:'accepted'|'preparing'|'indexing'|'paused'|'ready'|'failed'|'unavailable';indexes:RecordValue[]};
/** Validate the native presentation boundary while preserving every field. This
 * never constructs a recovery roster, a grant or native mutation authority. */
export function validateRebuildObservation(input:unknown):NativeRebuildObservation {
    const value=record(input);
    if(value.schema!==1||!['accepted','preparing','indexing','paused','ready','failed','unavailable'].includes(String(value.stage))
        ||value.provenance!=='native_rebuild_from_saved_configuration'||Buffer.byteLength(JSON.stringify(value))>512*1024
        ||!Array.isArray(value.indexes)||!value.indexes.length||value.indexes.length>64)throw new ServiceError('invalid_native_rebuild_observation',false,value);
    identifier(value.operation_id,16);identifier(value.candidate,32);identifier(value.predecessor,32);identifier(value.original_namespace,16);
    if(value.successor_namespace!==null)identifier(value.successor_namespace,16);
    if(value.index_scope!==null)identifier(value.index_scope,32);
    const location=(input:unknown)=>{if(typeof input!=='string'||!input||input.length>4096||/[\u0000-\u001f\u007f]/.test(input))throw new ServiceError('invalid_native_rebuild_location',false,value);};
    for(const field of ['preserved_state','preserved_control','successor_state','successor_control'])location(value[field]);
    if(value.preserved_state===value.successor_state||value.preserved_control===value.successor_control||!Array.isArray(value.preserved_index_roots)||value.preserved_index_roots.length>64)throw new ServiceError('invalid_native_rebuild_observation',false,value);
    value.preserved_index_roots.forEach(location);
    for(const input of value.indexes){
        const index=record(input),original=record(index.original);
        identifier(original.binding,32);identifier(index.settings_sha256,32);location(original.root);location(original.data_root);
        if(typeof original.collection!=='string'||!original.collection||typeof index.original_paused!=='boolean'||typeof index.paused!=='boolean'||typeof index.watching!=='boolean'
            ||!['pending','paused','ready','failed','unavailable'].includes(String(index.readiness)))throw new ServiceError('invalid_native_rebuild_index',false,value);
        const sources=record(index.sources);if(Object.keys(sources).length>16)throw new ServiceError('invalid_native_rebuild_index',false,value);Object.values(sources).forEach(location);
        if(index.replacement!==null){const replacement=record(index.replacement);identifier(replacement.binding,32);location(replacement.data_root);
            if(replacement.root!==original.root||replacement.collection!==original.collection||replacement.data_root===original.data_root)throw new ServiceError('invalid_native_rebuild_index',false,value);}
        if(index.job!==null){const job=record(index.job);identifier(job.id,32);if(job.project!==record(index.replacement).binding)throw new ServiceError('invalid_native_rebuild_job',false,value);}
    }
    return value as NativeRebuildObservation;
}

export function rebuildNotice(input:unknown):{key:string;message:string;warning:boolean;value:NativeRebuildObservation}{
    const value=validateRebuildObservation(input),distinct=['ready','paused','failed','unavailable'].includes(value.stage),stage=distinct?value.stage:'started';
    const message=value.stage==='ready'?'AIRK rebuilt replacement indexes. Previous state is preserved. Saved agent connections require reconnection.'
        :value.stage==='paused'?'AIRK preserved previous state. Replacement indexing is paused; review its saved and current preferences in Management.'
        :value.stage==='failed'?'AIRK could not finish rebuilding replacement indexes. Previous state and the failure details are preserved. Review Management → Service.'
        :value.stage==='unavailable'?'AIRK cannot verify the historical rebuild outcome from the retained job metadata. Previous state is preserved. Check current index readiness separately in Management.'
        :value.stage==='accepted'?'AIRK accepted a rebuild from the saved source folders and settings. Previous state is preserved; indexing starts after native preparation.'
        :value.stage==='preparing'?'AIRK is preparing replacement indexes from the saved source folders and settings. Previous state is preserved.'
        :'AIRK is rebuilding replacement indexes from the saved source folders and settings. Previous state is preserved; search becomes available as indexing completes.';
    return {key:`${value.operation_id}:${stage}`,message,warning:value.stage==='failed',value};
}

export interface RebuildNoticeStorage {get<T>(key:string):T|undefined;update(key:string,value:unknown):PromiseLike<void>}
export class RebuildNotices {
    private writes:Promise<void>=Promise.resolve();
    constructor(private readonly storage:RebuildNoticeStorage){}
    report(input:unknown,show:(message:string,warning:boolean)=>void):Promise<void>{
        const notice=rebuildNotice(input);
        const write=this.writes.then(async()=>{
            const saved=this.storage.get<unknown>('airk.rebuildNotices.1');
            if(saved!==undefined&&(!Array.isArray(saved)||saved.length>256||saved.some(key=>typeof key!=='string'||!/^[a-f0-9]{32}:(started|paused|ready|failed|unavailable)$/.test(key))))throw new ServiceError('invalid_saved_rebuild_notices');
            const keys=new Set<string>(saved as string[]|undefined);
            if(keys.has(notice.key))return;
            if(keys.size>=256)throw new ServiceError('rebuild_notice_capacity');
            await this.storage.update('airk.rebuildNotices.1',[...keys,notice.key]);
            show(notice.message,notice.warning);
        });
        this.writes=write.catch(()=>{});return write;
    }
}
